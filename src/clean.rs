use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::summary::TableSummary;

/// Result of cleaning a single CSV file.
pub struct CleanReport {
    pub input: String,
    pub output: PathBuf,
    pub rows_total: usize,
    pub rows_with_missing: usize,
    pub dropped_columns: Vec<String>,
    pub kept_columns: usize,
}

/// Derive a default output path for a cleaned file: `name.csv` -> `name.clean.csv`.
pub fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "csv".to_string());
    let file_name = format!("{stem}.clean.{ext}");
    match input.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

/// Given a summary already computed over the file, find the indices (and names) of
/// columns that are 100% empty (every row missing that field).
pub fn find_empty_columns(summary: &TableSummary) -> Vec<(usize, String)> {
    summary
        .columns
        .iter()
        .enumerate()
        .filter(|(_, col)| col.count > 0 && col.missing == col.count)
        .map(|(i, col)| (i, col.name.clone()))
        .collect()
}

/// Stream the input CSV a second time, dropping the given column indices and writing
/// the remaining columns to `output_path`. Any row that still has a missing field in
/// one of the kept columns is logged as a WARNING and counted.
pub fn write_clean_csv(
    path: &Path,
    delim_byte: u8,
    has_header: bool,
    summary: &TableSummary,
    output_path: &Path,
) -> Result<CleanReport> {
    let dropped = find_empty_columns(summary);
    let dropped_indices: Vec<usize> = dropped.iter().map(|(i, _)| *i).collect();
    let dropped_names: Vec<String> = dropped.into_iter().map(|(_, n)| n).collect();

    let kept_indices: Vec<usize> = (0..summary.columns.len())
        .filter(|i| !dropped_indices.contains(i))
        .collect();
    let kept_names: Vec<&str> = kept_indices
        .iter()
        .map(|&i| summary.columns[i].name.as_str())
        .collect();

    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delim_byte)
        .has_headers(has_header)
        .flexible(true)
        .from_reader(file);

    let mut writer = csv::WriterBuilder::new()
        .delimiter(delim_byte)
        .from_path(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;

    if has_header {
        writer
            .write_record(kept_names.iter())
            .context("failed to write cleaned header row")?;
    }

    let mut rows_total = 0usize;
    let mut rows_with_missing = 0usize;

    for (row_idx, result) in reader.records().enumerate() {
        let record =
            result.with_context(|| format!("failed to read a row in {}", path.display()))?;
        rows_total += 1;

        let mut missing_in_row: Vec<String> = Vec::new();
        let mut out_fields: Vec<&str> = Vec::with_capacity(kept_indices.len());

        for &i in &kept_indices {
            let raw = record.get(i).unwrap_or("");
            if raw.trim().is_empty() {
                let name = summary
                    .columns
                    .get(i)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                missing_in_row.push(name.to_string());
            }
            out_fields.push(raw);
        }

        if !missing_in_row.is_empty() {
            rows_with_missing += 1;
            // Data row number as it appears in the file (1-indexed, accounting for header).
            let display_row = row_idx + 1 + usize::from(has_header);
            eprintln!(
                "WARNING: {} row {}: missing value in column(s): {}",
                path.display(),
                display_row,
                missing_in_row.join(", ")
            );
        }

        writer
            .write_record(out_fields.iter())
            .with_context(|| format!("failed to write row to {}", output_path.display()))?;
    }

    writer
        .flush()
        .with_context(|| format!("failed to flush {}", output_path.display()))?;

    Ok(CleanReport {
        input: path.display().to_string(),
        output: output_path.to_path_buf(),
        rows_total,
        rows_with_missing,
        dropped_columns: dropped_names,
        kept_columns: kept_indices.len(),
    })
}
