use crate::clean::CleanReport;
use crate::drift::{DriftMode, DriftProfile, DriftReport, NumericDriftKind};
use crate::schema::{ComplianceReport, Schema};
use crate::summary::TableSummary;
use crate::transform::TransformReport;

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n:.3}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Render a (potentially long) list of category values as a comma-joined,
/// length-capped preview: at most `max_items` values, each truncated.
fn preview_list(values: &[String], max_items: usize) -> String {
    let shown: Vec<String> = values
        .iter()
        .take(max_items)
        .map(|v| truncate(v, 24))
        .collect();
    let mut out = shown.join(", ");
    if values.len() > max_items {
        out.push_str(&format!(", … (+{} more)", values.len() - max_items));
    }
    out
}

pub fn print_summary(summary: &TableSummary, top_n: usize) {
    println!("File:    {}", summary.file);
    println!("Rows:    {}", summary.rows);
    println!("Columns: {}", summary.columns.len());
    println!();

    for col in &summary.columns {
        let kind = if col.is_numeric() { "numeric" } else { "text" };
        println!("── {} ({kind})", col.name);
        println!(
            "   present: {} / {}    missing: {} ({:.1}%)    distinct: {}{}",
            col.present(),
            col.count,
            col.missing,
            if col.count > 0 {
                100.0 * col.missing as f64 / col.count as f64
            } else {
                0.0
            },
            col.distinct_count(),
            if col.distinct_overflow { "+" } else { "" }
        );

        if col.is_numeric() {
            if let (Some(min), Some(max), Some(mean)) = (col.min, col.max, col.mean()) {
                print!(
                    "   min: {}    max: {}    mean: {}",
                    fmt_num(min),
                    fmt_num(max),
                    fmt_num(mean)
                );
                if let Some(sd) = col.stddev() {
                    print!("    stddev: {}", fmt_num(sd));
                }
                println!();
            }
        } else if let (Some(min_len), Some(max_len)) = (col.min_len, col.max_len) {
            println!("   min len: {min_len}    max len: {max_len}");
        }

        if top_n > 0 && col.present() > 0 {
            let top = col.top_values(top_n);
            let parts: Vec<String> = top
                .iter()
                .map(|(v, c)| format!("{:?}×{}", truncate(v, 24), c))
                .collect();
            println!("   top: {}", parts.join(", "));
        }
        println!();
    }
}

pub fn print_clean_summary(report: &CleanReport) {
    println!("Clean summary for {}", report.input);
    println!("  output file:      {}", report.output.display());
    println!("  rows processed:   {}", report.rows_total);
    println!(
        "  rows with nulls:  {} ({:.1}%)",
        report.rows_with_missing,
        if report.rows_total > 0 {
            100.0 * report.rows_with_missing as f64 / report.rows_total as f64
        } else {
            0.0
        }
    );
    println!(
        "  columns dropped:  {}{}",
        report.dropped_columns.len(),
        if report.dropped_columns.is_empty() {
            String::new()
        } else {
            format!(" ({})", report.dropped_columns.join(", "))
        }
    );
    println!("  columns kept:     {}", report.kept_columns);
}

pub fn print_transform_summary(report: &TransformReport) {
    println!("Transform summary for {}", report.input);
    println!("  output file:    {}", report.output.display());
    println!("  steps applied:  {}", report.steps_applied);
    println!("  rows in:        {}", report.rows_in);
    println!(
        "  rows out:       {} ({:.1}%)",
        report.rows_out,
        if report.rows_in > 0 {
            100.0 * report.rows_out as f64 / report.rows_in as f64
        } else {
            0.0
        }
    );
}

pub fn print_schema(file: &str, schema: &Schema, saved_to: Option<&std::path::Path>) {
    println!("Inferred schema for {file}");
    println!("  columns: {}", schema.columns.len());
    if let Some(path) = saved_to {
        println!("  saved to: {}", path.display());
    }
    println!();

    let name_width = schema
        .columns
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0);
    for col in &schema.columns {
        println!(
            "  {:<name_width$}  {:<7}  {}",
            col.name,
            col.col_type.to_string(),
            if col.nullable { "nullable" } else { "not null" },
        );
    }
}

pub fn print_drift_baseline(file: &str, profile: &DriftProfile, saved_to: &std::path::Path) {
    println!("Drift baseline for {file}");
    println!("  rows: {}", profile.rows);
    println!("  saved to: {}", saved_to.display());
    println!();

    for col in &profile.columns {
        if let Some(num) = &col.numeric {
            println!(
                "  {}: min: {}    max: {}    mean: {}",
                col.name,
                fmt_num(num.min),
                fmt_num(num.max),
                fmt_num(num.mean)
            );
        } else if let Some(cat) = &col.categorical {
            println!(
                "  {}: categories: {}{}",
                col.name,
                cat.distinct_count,
                if cat.capped { "+" } else { "" }
            );
        }
    }
}

pub fn print_drift_report(report: &DriftReport, baseline_path: &std::path::Path) {
    println!("Drift check for {}", report.file);
    println!("  against: {}", baseline_path.display());
    println!(
        "  mode: {}",
        match report.mode {
            DriftMode::Fail => "fail",
            DriftMode::Warn => "warn",
        }
    );

    let label = if !report.has_drift() {
        "NO DRIFT"
    } else if report.mode == DriftMode::Warn {
        "WARNING: DRIFT DETECTED"
    } else {
        "DRIFT DETECTED"
    };
    println!("  status: {label}");

    if !report.has_drift() {
        return;
    }
    println!();

    if !report.numeric_drifts.is_empty() {
        println!("  numeric range drift:");
        for d in &report.numeric_drifts {
            let kind = match d.kind {
                NumericDriftKind::BelowMin => "below min",
                NumericDriftKind::AboveMax => "above max",
            };
            println!(
                "    {}: observed {} is {} (baseline range: {}..{})",
                d.column,
                fmt_num(d.observed),
                kind,
                fmt_num(d.baseline_min),
                fmt_num(d.baseline_max)
            );
        }
    }

    if !report.categorical_drifts.is_empty() {
        println!("  categorical cardinality drift:");
        for d in &report.categorical_drifts {
            println!(
                "    {}: baseline {} categories, observed {} categories ({:.1}% change)",
                d.column, d.baseline_distinct, d.observed_distinct, d.pct_change
            );
            if !d.new_categories.is_empty() {
                println!("      new: {}", preview_list(&d.new_categories, 10));
            }
            if !d.missing_categories.is_empty() {
                println!("      missing: {}", preview_list(&d.missing_categories, 10));
            }
        }
    }
}

pub fn print_compliance(report: &ComplianceReport, schema_path: &std::path::Path) {
    println!("Schema check for {}", report.file);
    println!("  against: {}", schema_path.display());
    println!(
        "  status: {}",
        if report.is_compliant() {
            "COMPLIANT"
        } else {
            "NOT COMPLIANT"
        }
    );

    if report.is_compliant() {
        return;
    }
    println!();

    if !report.missing_columns.is_empty() {
        println!("  missing columns: {}", report.missing_columns.join(", "));
    }
    if !report.extra_columns.is_empty() {
        println!("  extra columns: {}", report.extra_columns.join(", "));
    }
    if !report.type_mismatches.is_empty() {
        println!("  type mismatches:");
        for (name, expected, actual) in &report.type_mismatches {
            println!("    {name}: expected {expected}, found {actual}");
        }
    }
    if !report.nullability_violations.is_empty() {
        println!("  nullability violations:");
        for name in &report.nullability_violations {
            println!("    {name}: expected not null, found missing values");
        }
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB ({bytes} bytes)", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB ({bytes} bytes)", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn print_parquet_check(report: &crate::check::ParquetCheckReport) {
    println!("Parquet check for {}", report.file);
    println!(
        "  status: {}",
        if report.is_valid() {
            "VALID"
        } else {
            "INVALID"
        }
    );
    println!("  size: {}", format_file_size(report.file_size_bytes));

    if !report.errors.is_empty() {
        println!("  errors:");
        for err in &report.errors {
            println!("    - {err}");
        }
    }

    if report.valid {
        println!("  rows: {}", report.rows);
        println!("  columns: {}", report.columns.len());

        if !report.columns.is_empty() {
            println!("  schema:");
            let name_width = report
                .columns
                .iter()
                .map(|c| {
                    if c.name.is_empty() {
                        "(unnamed)".len()
                    } else {
                        c.name.chars().count()
                    }
                })
                .max()
                .unwrap_or(0);

            for col in &report.columns {
                let display_name = if col.name.is_empty() {
                    "(unnamed)"
                } else {
                    &col.name
                };
                let null_desc = if col.null_count == 0 {
                    "0 nulls".to_string()
                } else if col.null_count == 1 {
                    format!("1 null, {:.1}%", col.null_pct)
                } else {
                    format!("{} nulls, {:.1}%", col.null_count, col.null_pct)
                };
                println!(
                    "    {:<name_width$}  {:<10}  ({})",
                    display_name, col.dtype, null_desc
                );
            }
        }
    }

    if !report.warnings.is_empty() {
        println!("  warnings:");
        for w in &report.warnings {
            println!("    - {w}");
        }
    }
}
