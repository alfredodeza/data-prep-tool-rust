use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::summary::TableSummary;

/// The inferred (or declared) type of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    Integer,
    Float,
    Text,
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ColumnType::Integer => "integer",
            ColumnType::Float => "float",
            ColumnType::Text => "text",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: ColumnType,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub delimiter: String,
    pub has_header: bool,
    pub columns: Vec<ColumnSchema>,
}

/// Infer a schema from a summary already computed over a CSV file.
pub fn infer_schema(summary: &TableSummary, delimiter: char, has_header: bool) -> Schema {
    let columns = summary
        .columns
        .iter()
        .map(|col| {
            let col_type = if col.present() == 0 {
                ColumnType::Text
            } else if col.is_integer() {
                ColumnType::Integer
            } else if col.is_numeric() {
                ColumnType::Float
            } else {
                ColumnType::Text
            };
            ColumnSchema {
                name: col.name.clone(),
                col_type,
                nullable: col.missing > 0,
            }
        })
        .collect();

    Schema {
        delimiter: delimiter.to_string(),
        has_header,
        columns,
    }
}

/// Derive a default schema output path for a CSV input: `name.csv` -> `name.schema.json`.
pub fn default_schema_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let file_name = format!("{stem}.schema.json");
    match input.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

pub fn save_schema(schema: &Schema, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(schema).context("failed to serialize schema")?;
    fs::write(path, json + "\n")
        .with_context(|| format!("failed to write schema to {}", path.display()))
}

pub fn load_schema(path: &Path) -> Result<Schema> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to open schema {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse schema {}", path.display()))
}

/// The result of comparing an actual (inferred) schema against an expected one.
pub struct ComplianceReport {
    pub file: String,
    pub missing_columns: Vec<String>,
    pub extra_columns: Vec<String>,
    pub type_mismatches: Vec<(String, ColumnType, ColumnType)>,
    pub nullability_violations: Vec<String>,
}

impl ComplianceReport {
    pub fn is_compliant(&self) -> bool {
        self.missing_columns.is_empty()
            && self.extra_columns.is_empty()
            && self.type_mismatches.is_empty()
            && self.nullability_violations.is_empty()
    }
}

/// A numeric type is compatible with a wider expected numeric type: an integer
/// column satisfies a schema that expects float, but not the other way around.
fn types_compatible(expected: ColumnType, actual: ColumnType) -> bool {
    expected == actual || (expected == ColumnType::Float && actual == ColumnType::Integer)
}

/// Compare an actual, freshly inferred schema against a previously saved one.
pub fn check_compliance(expected: &Schema, actual: &Schema, file: &str) -> ComplianceReport {
    let mut missing_columns = Vec::new();
    let mut type_mismatches = Vec::new();
    let mut nullability_violations = Vec::new();

    for exp_col in &expected.columns {
        match actual.columns.iter().find(|c| c.name == exp_col.name) {
            None => missing_columns.push(exp_col.name.clone()),
            Some(act_col) => {
                if !types_compatible(exp_col.col_type, act_col.col_type) {
                    type_mismatches.push((
                        exp_col.name.clone(),
                        exp_col.col_type,
                        act_col.col_type,
                    ));
                }
                if !exp_col.nullable && act_col.nullable {
                    nullability_violations.push(exp_col.name.clone());
                }
            }
        }
    }

    let extra_columns: Vec<String> = actual
        .columns
        .iter()
        .filter(|c| !expected.columns.iter().any(|e| e.name == c.name))
        .map(|c| c.name.clone())
        .collect();

    ComplianceReport {
        file: file.to_string(),
        missing_columns,
        extra_columns,
        type_mismatches,
        nullability_violations,
    }
}
