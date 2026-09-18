use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};
use polars::prelude::*;

/// Supported formats for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CheckFormat {
    Parquet,
    Csv,
}

impl CheckFormat {
    /// Attempt to infer the format from a file path's extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "parquet" | "pq" => Some(CheckFormat::Parquet),
            "csv" => Some(CheckFormat::Csv),
            _ => None,
        }
    }
}

/// Verification details for a single column in a Parquet file.
#[derive(Debug, Clone)]
pub struct ColumnCheck {
    pub name: String,
    pub dtype: String,
    pub null_count: usize,
    pub null_pct: f64,
}

/// Complete verification report for a Parquet file.
#[derive(Debug, Clone)]
pub struct ParquetCheckReport {
    pub file: String,
    pub file_size_bytes: u64,
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub rows: usize,
    pub columns: Vec<ColumnCheck>,
}

impl ParquetCheckReport {
    pub fn is_valid(&self) -> bool {
        self.valid && self.errors.is_empty()
    }
}

/// Convert a Polars result into an anyhow result.
fn polars_result<T>(r: PolarsResult<T>) -> Result<T> {
    r.map_err(anyhow::Error::from)
}

/// Verify a Parquet file:
/// 1. File accessibility and size check (minimum 8 bytes for PAR1 header & footer)
/// 2. Header and footer magic bytes (`PAR1`)
/// 3. Readability and decode validation across all row groups via `ParquetReader`
/// 4. Schema, row count, column count, null counts, and empty column detection
pub fn verify_parquet(path: &Path) -> Result<ParquetCheckReport> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to read metadata of {}", path.display()))?;
    let file_size = metadata.len();

    let mut report = ParquetCheckReport {
        file: path.display().to_string(),
        file_size_bytes: file_size,
        valid: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        rows: 0,
        columns: Vec::new(),
    };

    // Parquet file must be at least 8 bytes (4 bytes PAR1 header + 4 bytes PAR1 footer)
    if file_size < 8 {
        report.valid = false;
        report.errors.push(format!(
            "file is too small to be a valid Parquet file (size: {file_size} bytes, minimum: 8 bytes)"
        ));
        return Ok(report);
    }

    // Check magic header
    let mut header = [0u8; 4];
    file.read_exact(&mut header)
        .with_context(|| format!("failed to read header of {}", path.display()))?;
    if &header != b"PAR1" {
        report.valid = false;
        report.errors.push(format!(
            "invalid magic header: expected 'PAR1', found '{}'",
            String::from_utf8_lossy(&header)
        ));
    }

    // Check magic footer
    let mut footer = [0u8; 4];
    file.seek(SeekFrom::End(-4))
        .with_context(|| format!("failed to seek to footer of {}", path.display()))?;
    file.read_exact(&mut footer)
        .with_context(|| format!("failed to read footer of {}", path.display()))?;
    if &footer != b"PAR1" {
        report.valid = false;
        report.errors.push(format!(
            "invalid magic footer: expected 'PAR1', found '{}'",
            String::from_utf8_lossy(&footer)
        ));
    }

    // If magic bytes are already corrupt, we stop before attempting decode
    if !report.valid {
        return Ok(report);
    }

    // Rewind and attempt full decode with ParquetReader to verify data integrity
    file.seek(SeekFrom::Start(0))
        .with_context(|| format!("failed to rewind {}", path.display()))?;

    let df = match polars_result(ParquetReader::new(file).finish()) {
        Ok(df) => df,
        Err(err) => {
            report.valid = false;
            report
                .errors
                .push(format!("failed to read parquet data: {err:#}"));
            return Ok(report);
        }
    };

    report.rows = df.height();
    let columns = df.columns();

    if columns.is_empty() {
        report.warnings.push("file has 0 columns".to_string());
    }

    if df.height() == 0 {
        report
            .warnings
            .push("file contains 0 rows (empty dataset)".to_string());
    }

    let mut seen_names = std::collections::HashSet::new();
    for (idx, col) in columns.iter().enumerate() {
        let name = col.name().to_string();
        if name.is_empty() {
            report
                .warnings
                .push(format!("column at index {idx} has an empty column name"));
        } else if !seen_names.insert(name.clone()) {
            report
                .warnings
                .push(format!("duplicate column name '{name}'"));
        }

        let dtype = format!("{}", col.dtype());
        let null_count = col.null_count();
        let null_pct = if df.height() > 0 {
            (null_count as f64 / df.height() as f64) * 100.0
        } else {
            0.0
        };

        if df.height() > 0 && null_count == df.height() {
            let label = if name.is_empty() {
                format!("column at index {idx} (unnamed)")
            } else {
                format!("column '{name}'")
            };
            report
                .warnings
                .push(format!("{label} is 100.0% null ({null_count} nulls)"));
        }

        report.columns.push(ColumnCheck {
            name,
            dtype,
            null_count,
            null_pct,
        });
    }

    Ok(report)
}
