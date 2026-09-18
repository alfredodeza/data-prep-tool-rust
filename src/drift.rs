use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::summary::TableSummary;

/// Numeric value-range captured for a numeric column at baseline time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericRange {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

/// Categorical fingerprint captured for a text column at baseline time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoricalProfile {
    /// Number of distinct values observed (may exceed `categories.len()` when capped).
    pub distinct_count: usize,
    /// The distinct values themselves, capped at `max_categories`, sorted for stable diffs.
    pub categories: Vec<String>,
    /// True when `categories` does not hold every distinct value observed.
    pub capped: bool,
}

/// The persisted drift profile of a single column: value-range for numeric columns,
/// or a categorical fingerprint for text columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDrift {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<NumericRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categorical: Option<CategoricalProfile>,
}

/// A drift baseline/profile for an entire CSV file, saved to disk and later
/// compared against freshly summarized data with `drift check`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftProfile {
    pub delimiter: String,
    pub has_header: bool,
    pub rows: usize,
    pub columns: Vec<ColumnDrift>,
}

/// Build a drift profile from a summary already computed over a CSV file.
pub fn build_profile(
    summary: &TableSummary,
    delimiter: char,
    has_header: bool,
    max_categories: usize,
) -> DriftProfile {
    let columns = summary
        .columns
        .iter()
        .map(|col| {
            if col.is_numeric() {
                let (min, max, mean) = match (col.min, col.max, col.mean()) {
                    (Some(min), Some(max), Some(mean)) => (min, max, mean),
                    _ => {
                        return ColumnDrift {
                            name: col.name.clone(),
                            numeric: None,
                            categorical: None,
                        }
                    }
                };
                ColumnDrift {
                    name: col.name.clone(),
                    numeric: Some(NumericRange { min, max, mean }),
                    categorical: None,
                }
            } else if col.present() > 0 {
                let mut categories: Vec<String> = col.value_counts.keys().cloned().collect();
                categories.sort();
                let distinct_count = col.distinct_count();
                let capped = col.distinct_overflow || categories.len() > max_categories;
                categories.truncate(max_categories);
                ColumnDrift {
                    name: col.name.clone(),
                    numeric: None,
                    categorical: Some(CategoricalProfile {
                        distinct_count,
                        categories,
                        capped,
                    }),
                }
            } else {
                ColumnDrift {
                    name: col.name.clone(),
                    numeric: None,
                    categorical: None,
                }
            }
        })
        .collect();

    DriftProfile {
        delimiter: delimiter.to_string(),
        has_header,
        rows: summary.rows,
        columns,
    }
}

/// Derive a default drift-profile output path: `name.csv` -> `name.drift.json`.
pub fn default_drift_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let file_name = format!("{stem}.drift.json");
    match input.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

pub fn save_profile(profile: &DriftProfile, path: &Path) -> Result<()> {
    let json =
        serde_json::to_string_pretty(profile).context("failed to serialize drift profile")?;
    fs::write(path, json + "\n")
        .with_context(|| format!("failed to write drift profile to {}", path.display()))
}

pub fn load_profile(path: &Path) -> Result<DriftProfile> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to open drift profile {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse drift profile {}", path.display()))
}

/// How a `drift check` should behave when drift is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DriftMode {
    /// Print findings and exit non-zero.
    Fail,
    /// Print findings but always exit zero.
    Warn,
}

#[derive(Debug, Clone)]
pub enum NumericDriftKind {
    BelowMin,
    AboveMax,
}

#[derive(Debug, Clone)]
pub struct NumericDrift {
    pub column: String,
    pub kind: NumericDriftKind,
    pub baseline_min: f64,
    pub baseline_max: f64,
    pub observed: f64,
}

#[derive(Debug, Clone)]
pub struct CategoricalDrift {
    pub column: String,
    pub baseline_distinct: usize,
    pub observed_distinct: usize,
    pub pct_change: f64,
    pub new_categories: Vec<String>,
    pub missing_categories: Vec<String>,
}

pub struct DriftReport {
    pub file: String,
    pub mode: DriftMode,
    pub numeric_drifts: Vec<NumericDrift>,
    pub categorical_drifts: Vec<CategoricalDrift>,
}

impl DriftReport {
    pub fn has_drift(&self) -> bool {
        !self.numeric_drifts.is_empty() || !self.categorical_drifts.is_empty()
    }

    /// Whether the process should exit non-zero for this report.
    pub fn should_fail(&self) -> bool {
        self.mode == DriftMode::Fail && self.has_drift()
    }
}

/// Compare a freshly built drift profile against a saved baseline.
pub fn check_drift(
    baseline: &DriftProfile,
    actual: &DriftProfile,
    file: &str,
    mode: DriftMode,
    tolerance_pct: f64,
    category_tolerance_pct: f64,
) -> DriftReport {
    let mut numeric_drifts = Vec::new();
    let mut categorical_drifts = Vec::new();

    for base_col in &baseline.columns {
        let Some(act_col) = actual.columns.iter().find(|c| c.name == base_col.name) else {
            continue;
        };

        if let (Some(base_num), Some(act_num)) = (&base_col.numeric, &act_col.numeric) {
            let range = base_num.max - base_num.min;
            let slack = range.abs() * (tolerance_pct / 100.0);

            if act_num.min < base_num.min - slack {
                numeric_drifts.push(NumericDrift {
                    column: base_col.name.clone(),
                    kind: NumericDriftKind::BelowMin,
                    baseline_min: base_num.min,
                    baseline_max: base_num.max,
                    observed: act_num.min,
                });
            }
            if act_num.max > base_num.max + slack {
                numeric_drifts.push(NumericDrift {
                    column: base_col.name.clone(),
                    kind: NumericDriftKind::AboveMax,
                    baseline_min: base_num.min,
                    baseline_max: base_num.max,
                    observed: act_num.max,
                });
            }
        }

        if let (Some(base_cat), Some(act_cat)) = (&base_col.categorical, &act_col.categorical) {
            let baseline_count = base_cat.distinct_count;
            let actual_count = act_cat.distinct_count;

            let pct_change = if baseline_count > 0 {
                (actual_count as f64 - baseline_count as f64).abs() / baseline_count as f64 * 100.0
            } else if actual_count > 0 {
                100.0
            } else {
                0.0
            };

            let (new_categories, missing_categories) = if !base_cat.capped && !act_cat.capped {
                let base_set: BTreeSet<&str> =
                    base_cat.categories.iter().map(|s| s.as_str()).collect();
                let act_set: BTreeSet<&str> =
                    act_cat.categories.iter().map(|s| s.as_str()).collect();
                let new: Vec<String> = act_set
                    .difference(&base_set)
                    .map(|s| s.to_string())
                    .collect();
                let missing: Vec<String> = base_set
                    .difference(&act_set)
                    .map(|s| s.to_string())
                    .collect();
                (new, missing)
            } else {
                (Vec::new(), Vec::new())
            };

            if pct_change > category_tolerance_pct {
                categorical_drifts.push(CategoricalDrift {
                    column: base_col.name.clone(),
                    baseline_distinct: baseline_count,
                    observed_distinct: actual_count,
                    pct_change,
                    new_categories,
                    missing_categories,
                });
            }
        }
    }

    DriftReport {
        file: file.to_string(),
        mode,
        numeric_drifts,
        categorical_drifts,
    }
}
