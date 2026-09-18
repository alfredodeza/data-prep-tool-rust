use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_csvsum"))
}

/// Unique path under the OS temp dir for a scratch input/output file used by a test.
/// `name` must include the desired extension, e.g. "input.csv".
fn scratch_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "csv"));
    p.push(format!(
        "csvsum-test-{}-{}-{}.{}",
        std::process::id(),
        stem,
        nanos,
        ext
    ));
    p
}

const WINE_FIXTURE: &str = "name,grape,region,variety,rating\n\
Wine A,Carignan,California,Red Wine,91.0\n\
Wine B,Zinfandel,California,Red Wine,89.0\n\
Wine C,,Oregon,White Wine,90.0\n\
Wine D,Merlot,Texas,Red Wine,95.0\n";

#[test]
fn transform_default_drops_grape_casts_rating_and_filters_90_and_above() {
    let input = scratch_path("wine-default-input.csv");
    let output = scratch_path("wine-default-output.csv");
    fs::write(&input, WINE_FIXTURE).unwrap();

    let out = bin()
        .arg("transform")
        .arg("default")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let contents = fs::read_to_string(&output).unwrap();
    let header = contents.lines().next().unwrap();
    // grape column is dropped
    assert_eq!(header, "name,region,variety,rating");

    // Wine B (89.0) is filtered out; the rest (91, 90, 95) survive as integers.
    assert!(contents.contains("Wine A,California,Red Wine,91\n"));
    assert!(!contents.contains("Wine B"));
    assert!(contents.contains("Wine C,Oregon,White Wine,90\n"));
    assert!(contents.contains("Wine D,Texas,Red Wine,95\n"));
    assert!(!contents.contains("91.0"));

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
}

#[test]
fn transform_apply_runs_equivalent_yaml_spec() {
    let input = scratch_path("wine-yaml-input.csv");
    let output = scratch_path("wine-yaml-output.csv");
    let spec_path = scratch_path("wine-spec.yaml");
    fs::write(&input, WINE_FIXTURE).unwrap();
    fs::write(
        &spec_path,
        "steps:\n\
         \x20\x20- step: drop_columns\n\
         \x20\x20\x20\x20columns: [grape]\n\
         \x20\x20- step: cast\n\
         \x20\x20\x20\x20column: rating\n\
         \x20\x20\x20\x20to: integer\n\
         \x20\x20- step: filter\n\
         \x20\x20\x20\x20column: rating\n\
         \x20\x20\x20\x20op: gte\n\
         \x20\x20\x20\x20value: 90\n",
    )
    .unwrap();

    let out = bin()
        .arg("transform")
        .arg("apply")
        .arg(&input)
        .arg("--spec")
        .arg(&spec_path)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let contents = fs::read_to_string(&output).unwrap();
    let header = contents.lines().next().unwrap();
    assert_eq!(header, "name,region,variety,rating");
    assert!(contents.contains("Wine A,California,Red Wine,91\n"));
    assert!(!contents.contains("Wine B"));
    assert!(contents.contains("Wine C,Oregon,White Wine,90\n"));
    assert!(contents.contains("Wine D,Texas,Red Wine,95\n"));

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
    fs::remove_file(&spec_path).ok();
}

#[test]
fn transform_apply_rejects_unknown_column_with_clear_error() {
    let input = scratch_path("wine-bad-column-input.csv");
    let spec_path = scratch_path("wine-bad-column-spec.yaml");
    fs::write(&input, WINE_FIXTURE).unwrap();
    fs::write(
        &spec_path,
        "steps:\n  - step: drop_columns\n    columns: [not_a_real_column]\n",
    )
    .unwrap();

    let out = bin()
        .arg("transform")
        .arg("apply")
        .arg(&input)
        .arg("--spec")
        .arg(&spec_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());

    fs::remove_file(&input).ok();
    fs::remove_file(&spec_path).ok();
}

#[test]
fn transform_scaffold_writes_a_parseable_yaml_template() {
    let output = scratch_path("scaffold.yaml");

    let out = bin()
        .arg("transform")
        .arg("scaffold")
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists());

    let contents = fs::read_to_string(&output).unwrap();
    // Should mention every supported step type so users don't have to
    // remember the variations by heart.
    for step in [
        "drop_columns",
        "select_columns",
        "rename_column",
        "cast",
        "filter",
    ] {
        assert!(contents.contains(step), "scaffold missing `{step}` step");
    }

    // And it must actually be valid, parseable YAML matching our spec shape.
    let parsed: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();
    assert!(parsed.get("steps").is_some());

    fs::remove_file(&output).ok();
}

#[test]
fn transform_scaffold_prints_to_stdout_without_output_flag() {
    let out = bin()
        .arg("transform")
        .arg("scaffold")
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("steps:"));
}

#[test]
fn transform_default_outputs_parquet_by_extension() {
    use polars::prelude::*;

    let input = scratch_path("wine-default-parquet.csv");
    let output = scratch_path("wine-default-output.parquet");
    fs::write(&input, WINE_FIXTURE).unwrap();

    let out = bin()
        .arg("transform")
        .arg("default")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&output).unwrap();
    assert!(bytes.len() >= 8, "file too small for parquet");
    assert_eq!(&bytes[..4], b"PAR1", "missing PAR1 magic header");
    assert_eq!(
        &bytes[bytes.len() - 4..],
        b"PAR1",
        "missing PAR1 magic footer"
    );

    let file = fs::File::open(&output).unwrap();
    let df = ParquetReader::new(file).finish().unwrap();
    assert_eq!(df.height(), 3);
    assert_eq!(df.width(), 4);
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(names, vec!["name", "region", "variety", "rating"]);

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
}

#[test]
fn transform_apply_outputs_parquet_by_extension() {
    use polars::prelude::*;

    let input = scratch_path("wine-apply-parquet.csv");
    let output = scratch_path("wine-apply-output.parquet");
    let spec_path = scratch_path("wine-apply-parquet-spec.yaml");
    fs::write(&input, WINE_FIXTURE).unwrap();
    fs::write(
        &spec_path,
        "steps:\n  - step: drop_columns\n    columns: [grape]\n",
    )
    .unwrap();

    let out = bin()
        .arg("transform")
        .arg("apply")
        .arg(&input)
        .arg("--spec")
        .arg(&spec_path)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&output).unwrap();
    assert!(bytes.len() >= 8, "file too small for parquet");
    assert_eq!(&bytes[..4], b"PAR1", "missing PAR1 magic header");
    assert_eq!(
        &bytes[bytes.len() - 4..],
        b"PAR1",
        "missing PAR1 magic footer"
    );

    let file = fs::File::open(&output).unwrap();
    let df = ParquetReader::new(file).finish().unwrap();
    assert_eq!(df.height(), 4);
    assert_eq!(df.width(), 4);

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
    fs::remove_file(&spec_path).ok();
}

#[test]
fn transform_default_outputs_parquet_with_output_format_flag() {
    use polars::prelude::*;

    let input = scratch_path("wine-format-flag.csv");
    let output = scratch_path("wine-format-flag-output.anyext");
    fs::write(&input, WINE_FIXTURE).unwrap();

    let out = bin()
        .arg("transform")
        .arg("default")
        .arg(&input)
        .arg("--output-format")
        .arg("parquet")
        .arg("--output")
        .arg(&output)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = fs::read(&output).unwrap();
    assert!(bytes.len() >= 8, "file too small for parquet");
    assert_eq!(&bytes[..4], b"PAR1", "missing PAR1 magic header");
    assert_eq!(
        &bytes[bytes.len() - 4..],
        b"PAR1",
        "missing PAR1 magic footer"
    );

    let file = fs::File::open(&output).unwrap();
    let df = ParquetReader::new(file).finish().unwrap();
    assert_eq!(df.height(), 3);

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
}

#[test]
fn transform_default_parquet_default_path_when_format_specified() {
    let input = scratch_path("wine-default-fmt.csv");
    fs::write(&input, WINE_FIXTURE).unwrap();

    let out = bin()
        .arg("transform")
        .arg("default")
        .arg(&input)
        .arg("--format")
        .arg("parquet")
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Expected default output path has .parquet extension
    let expected_output = input.parent().unwrap().join(format!(
        "{}.transformed.parquet",
        input.file_stem().unwrap().to_string_lossy()
    ));
    assert!(
        expected_output.exists(),
        "expected output file {:?} does not exist",
        expected_output
    );

    let bytes = fs::read(&expected_output).unwrap();
    assert_eq!(&bytes[..4], b"PAR1");

    fs::remove_file(&input).ok();
    fs::remove_file(&expected_output).ok();
}
