mod check;
mod clean;
mod display;
mod drift;
mod schema;
mod summary;
mod transform;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs::File;
use std::path::PathBuf;

use drift::DriftMode;
use summary::{ColumnStats, TableSummary};

/// csvsum: quick exploratory summaries of CSV files.
#[derive(Parser, Debug)]
#[command(name = "csvsum", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// One or more CSV files to summarize (ignored when a subcommand is used)
    files: Vec<PathBuf>,

    /// Field delimiter
    #[arg(short, long, default_value = ",")]
    delimiter: char,

    /// Number of most frequent values to show per column (0 to disable)
    #[arg(short, long, default_value_t = 5)]
    top: usize,

    /// Treat the file as having no header row (columns named column_1, column_2, ...)
    #[arg(long)]
    no_header: bool,

    /// Produce a cleaned CSV: drop columns that are 100% empty and warn on any
    /// remaining row that still has a missing field.
    #[arg(long)]
    clean: bool,

    /// Output path for the cleaned CSV (only valid with a single input file and --clean).
    /// Defaults to "<name>.clean.<ext>" next to the input file.
    #[arg(short, long, requires = "clean")]
    output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Infer or check a CSV's column schema.
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },
    /// Capture or check a data-drift baseline (value ranges and categorical fingerprints).
    Drift {
        #[command(subcommand)]
        action: DriftAction,
    },
    /// Transform a CSV file into a new CSV file using Polars.
    Transform {
        #[command(subcommand)]
        action: TransformAction,
    },
    /// Check and verify a data file (currently supports Parquet).
    Check {
        /// File to check and verify
        file: PathBuf,

        /// Explicitly specify file format instead of inferring from extension
        #[arg(short = 'f', long = "format", value_enum)]
        format: Option<check::CheckFormat>,
    },
}

#[derive(Subcommand, Debug)]
enum TransformAction {
    /// Apply the built-in wine-ratings transform: drop the `grape` column,
    /// cast `rating` from float to integer, and keep only entries rated 90
    /// and above.
    Default {
        /// CSV file to transform
        file: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,

        /// Output path for the transformed file. Defaults to
        /// "<name>.transformed.<ext>" next to the input file.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format (csv or parquet). Inferred from output extension if omitted.
        #[arg(short = 'f', long = "output-format", alias = "format", value_enum)]
        output_format: Option<transform::OutputFormat>,
    },
    /// Apply a declarative transform pipeline loaded from a YAML spec (see
    /// `transform scaffold` to generate a starting template).
    Apply {
        /// CSV file to transform
        file: PathBuf,

        /// Path to a YAML transform spec
        #[arg(short, long)]
        spec: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,

        /// Output path for the transformed file. Defaults to
        /// "<name>.transformed.<ext>" next to the input file.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format (csv or parquet). Inferred from output extension if omitted.
        #[arg(short = 'f', long = "output-format", alias = "format", value_enum)]
        output_format: Option<transform::OutputFormat>,
    },
    /// Write a YAML transform spec scaffold documenting every available step
    /// type, so you don't have to remember the exact shape of each one.
    Scaffold {
        /// Where to write the scaffold. Prints to stdout if omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum DriftAction {
    /// Capture a drift baseline from a CSV file and save it to disk: numeric
    /// min/max/mean ranges plus categorical value fingerprints for text columns.
    Baseline {
        /// CSV file to build the baseline from
        file: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,

        /// Where to save the drift baseline JSON. Defaults to "<name>.drift.json"
        /// next to the input file.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Max number of distinct values to track exactly per text column before
        /// the baseline is considered "capped" for that column (new/missing value
        /// diffing is skipped for capped columns, but cardinality drift still works).
        #[arg(long, default_value_t = 1000)]
        max_categories: usize,
    },
    /// Check a CSV file against a previously saved drift baseline.
    Check {
        /// CSV file to check
        file: PathBuf,

        /// Path to a drift baseline JSON file (as produced by `drift baseline`)
        #[arg(short, long)]
        baseline: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,

        /// WARNING mode: print drift findings but always exit 0 instead of
        /// failing with a non-zero exit code.
        #[arg(long, value_enum, default_value_t = DriftMode::Fail)]
        mode: DriftMode,

        /// Allowed numeric range drift, as a percentage of the baseline's range,
        /// before it is reported (default 0: any excursion outside the baseline
        /// min/max is flagged).
        #[arg(long, default_value_t = 0.0)]
        tolerance: f64,

        /// Allowed categorical cardinality change, as a percentage of the
        /// baseline's distinct-value count, before it is flagged as drift.
        #[arg(long, default_value_t = 20.0)]
        category_tolerance: f64,
    },
}

#[derive(Subcommand, Debug)]
enum SchemaAction {
    /// Infer a schema from a CSV file and save it to disk.
    Infer {
        /// CSV file to infer a schema from
        file: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,

        /// Where to save the schema JSON. Defaults to "<name>.schema.json" next to the input file.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check a CSV file against a previously saved schema.
    Check {
        /// CSV file to check
        file: PathBuf,

        /// Path to a schema JSON file (as produced by `schema infer`)
        #[arg(short, long)]
        schema: PathBuf,

        /// Field delimiter
        #[arg(short, long, default_value = ",")]
        delimiter: char,

        /// Treat the file as having no header row (columns named column_1, column_2, ...)
        #[arg(long)]
        no_header: bool,
    },
}

fn delimiter_byte(delimiter: char) -> Result<u8> {
    if !delimiter.is_ascii() {
        anyhow::bail!("delimiter must be a single ASCII character");
    }
    Ok(delimiter as u8)
}

fn summarize_file(path: &PathBuf, delim_byte: u8, has_header: bool) -> Result<TableSummary> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delim_byte)
        .has_headers(has_header)
        .flexible(true)
        .from_reader(file);

    let headers: Vec<String> = if has_header {
        reader
            .headers()
            .context("failed to read CSV header row")?
            .iter()
            .map(|h| h.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let mut columns: Vec<ColumnStats> = headers
        .iter()
        .map(|h| ColumnStats::new(h.clone()))
        .collect();

    let mut rows = 0usize;
    for result in reader.records() {
        let record =
            result.with_context(|| format!("failed to read a row in {}", path.display()))?;

        // Grow the column list on the fly for ragged / headerless files.
        while columns.len() < record.len() {
            let idx = columns.len() + 1;
            columns.push(ColumnStats::new(format!("column_{idx}")));
        }

        for (i, col) in columns.iter_mut().enumerate() {
            let raw = record.get(i).unwrap_or("");
            col.observe(raw);
        }
        rows += 1;
    }

    Ok(TableSummary {
        file: path.display().to_string(),
        rows,
        columns,
    })
}

fn run_schema_infer(
    file: &PathBuf,
    delimiter: char,
    no_header: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;

    let summary = summarize_file(file, delim_byte, has_header)?;
    let inferred = schema::infer_schema(&summary, delimiter, has_header);

    let output_path = output.unwrap_or_else(|| schema::default_schema_path(file));
    schema::save_schema(&inferred, &output_path)?;

    display::print_schema(&file.display().to_string(), &inferred, Some(&output_path));
    Ok(())
}

fn run_schema_check(
    file: &PathBuf,
    schema_path: &std::path::Path,
    delimiter: char,
    no_header: bool,
) -> Result<bool> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;

    let expected = schema::load_schema(schema_path)?;
    let summary = summarize_file(file, delim_byte, has_header)?;
    let actual = schema::infer_schema(&summary, delimiter, has_header);

    let report = schema::check_compliance(&expected, &actual, &file.display().to_string());
    let compliant = report.is_compliant();
    display::print_compliance(&report, schema_path);
    Ok(compliant)
}

fn run_drift_baseline(
    file: &PathBuf,
    delimiter: char,
    no_header: bool,
    output: Option<PathBuf>,
    max_categories: usize,
) -> Result<()> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;

    let summary = summarize_file(file, delim_byte, has_header)?;
    let profile = drift::build_profile(&summary, delimiter, has_header, max_categories);

    let output_path = output.unwrap_or_else(|| drift::default_drift_path(file));
    drift::save_profile(&profile, &output_path)?;

    display::print_drift_baseline(&file.display().to_string(), &profile, &output_path);
    Ok(())
}

fn run_drift_check(
    file: &PathBuf,
    baseline_path: &std::path::Path,
    delimiter: char,
    no_header: bool,
    mode: DriftMode,
    tolerance: f64,
    category_tolerance: f64,
) -> Result<bool> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;

    let baseline = drift::load_profile(baseline_path)?;
    let summary = summarize_file(file, delim_byte, has_header)?;
    // Don't cap the freshly observed categories: cardinality and new/missing
    // value diffing need the full set, bounded only by ColumnStats' own cap.
    let actual = drift::build_profile(&summary, delimiter, has_header, usize::MAX);

    let report = drift::check_drift(
        &baseline,
        &actual,
        &file.display().to_string(),
        mode,
        tolerance,
        category_tolerance,
    );
    let should_fail = report.should_fail();
    display::print_drift_report(&report, baseline_path);
    Ok(!should_fail)
}

fn resolve_transform_output(
    file: &std::path::Path,
    output: Option<PathBuf>,
    output_format: Option<transform::OutputFormat>,
) -> (PathBuf, transform::OutputFormat) {
    let format = output_format
        .or_else(|| {
            output
                .as_ref()
                .and_then(|p| transform::OutputFormat::from_path(p))
        })
        .unwrap_or(transform::OutputFormat::Csv);

    let output_path = output.unwrap_or_else(|| transform::default_output_path(file, format));
    (output_path, format)
}

fn run_transform_default(
    file: &std::path::Path,
    delimiter: char,
    no_header: bool,
    output: Option<PathBuf>,
    output_format: Option<transform::OutputFormat>,
) -> Result<()> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;
    let (output_path, format) = resolve_transform_output(file, output, output_format);

    let report = transform::run_default(file, delim_byte, has_header, &output_path, format)?;
    display::print_transform_summary(&report);
    Ok(())
}

fn run_transform_apply(
    file: &std::path::Path,
    spec_path: &std::path::Path,
    delimiter: char,
    no_header: bool,
    output: Option<PathBuf>,
    output_format: Option<transform::OutputFormat>,
) -> Result<()> {
    let delim_byte = delimiter_byte(delimiter)?;
    let has_header = !no_header;
    let (output_path, format) = resolve_transform_output(file, output, output_format);

    let spec = transform::load_spec(spec_path)?;
    let report = transform::run_spec(file, delim_byte, has_header, &spec, &output_path, format)?;
    display::print_transform_summary(&report);
    Ok(())
}

fn run_check(file: &std::path::Path, format: Option<check::CheckFormat>) -> Result<bool> {
    let resolved_format = match format {
        Some(f) => f,
        None => check::CheckFormat::from_path(file).ok_or_else(|| {
            anyhow::anyhow!(
                "cannot infer file format for '{}'; only .parquet / .pq files are supported (or specify --format parquet)",
                file.display()
            )
        })?,
    };

    match resolved_format {
        check::CheckFormat::Parquet => {
            let report = check::verify_parquet(file)?;
            display::print_parquet_check(&report);
            Ok(report.is_valid())
        }
        check::CheckFormat::Csv => {
            anyhow::bail!(
                "format 'csv' is not currently supported for check; check currently verifies Parquet files"
            );
        }
    }
}

fn run_transform_scaffold(output: Option<PathBuf>) -> Result<()> {
    match output {
        Some(path) => {
            transform::write_scaffold(&path)?;
            println!("Wrote transform spec scaffold to {}", path.display());
        }
        None => print!("{}", transform::scaffold_yaml()),
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Command::Check { file, format }) = cli.command {
        let ok = run_check(&file, format)?;
        if !ok {
            std::process::exit(1);
        }
        return Ok(());
    }

    if let Some(Command::Schema { action }) = cli.command {
        return match action {
            SchemaAction::Infer {
                file,
                delimiter,
                no_header,
                output,
            } => run_schema_infer(&file, delimiter, no_header, output),
            SchemaAction::Check {
                file,
                schema,
                delimiter,
                no_header,
            } => {
                let compliant = run_schema_check(&file, &schema, delimiter, no_header)?;
                if !compliant {
                    std::process::exit(1);
                }
                Ok(())
            }
        };
    }

    if let Some(Command::Drift { action }) = cli.command {
        return match action {
            DriftAction::Baseline {
                file,
                delimiter,
                no_header,
                output,
                max_categories,
            } => run_drift_baseline(&file, delimiter, no_header, output, max_categories),
            DriftAction::Check {
                file,
                baseline,
                delimiter,
                no_header,
                mode,
                tolerance,
                category_tolerance,
            } => {
                let ok = run_drift_check(
                    &file,
                    &baseline,
                    delimiter,
                    no_header,
                    mode,
                    tolerance,
                    category_tolerance,
                )?;
                if !ok {
                    std::process::exit(1);
                }
                Ok(())
            }
        };
    }

    if let Some(Command::Transform { action }) = cli.command {
        return match action {
            TransformAction::Default {
                file,
                delimiter,
                no_header,
                output,
                output_format,
            } => run_transform_default(&file, delimiter, no_header, output, output_format),
            TransformAction::Apply {
                file,
                spec,
                delimiter,
                no_header,
                output,
                output_format,
            } => run_transform_apply(&file, &spec, delimiter, no_header, output, output_format),
            TransformAction::Scaffold { output } => run_transform_scaffold(output),
        };
    }

    if cli.files.is_empty() {
        anyhow::bail!("no CSV files given (use `csvsum <FILES>...` or `csvsum schema`)");
    }

    if cli.output.is_some() && cli.files.len() > 1 {
        anyhow::bail!("--output can only be used with a single input file");
    }

    let delim_byte = delimiter_byte(cli.delimiter)?;
    let has_header = !cli.no_header;

    let mut had_error = false;
    for (i, file) in cli.files.iter().enumerate() {
        if i > 0 {
            println!();
        }
        match summarize_file(file, delim_byte, has_header) {
            Ok(summary) => {
                display::print_summary(&summary, cli.top);

                if cli.clean {
                    let output_path = cli
                        .output
                        .clone()
                        .unwrap_or_else(|| clean::default_output_path(file));
                    match clean::write_clean_csv(
                        file,
                        delim_byte,
                        has_header,
                        &summary,
                        &output_path,
                    ) {
                        Ok(report) => {
                            println!();
                            display::print_clean_summary(&report);
                        }
                        Err(e) => {
                            eprintln!("error: failed to clean {} ({e:#})", file.display());
                            had_error = true;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {} ({e:#})", file.display());
                had_error = true;
            }
        }
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}
