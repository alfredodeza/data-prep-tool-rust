use std::fs;
use std::path::PathBuf;
use std::process::Command;

use polars::prelude::*;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_csvsum"))
}

fn scratch_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "parquet"));
    p.push(format!(
        "csvsum-check-test-{}-{}-{}.{}",
        std::process::id(),
        stem,
        nanos,
        ext
    ));
    p
}

fn create_sample_parquet(path: &std::path::Path) {
    let mut df = df!(
        "name" => &["Alice", "Bob", "Charlie"],
        "age" => &[Some(25), Some(30), None],
        "active" => &[Some(true), Some(false), Some(true)]
    )
    .unwrap();
    let file = fs::File::create(path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();
}

#[test]
fn check_verifies_valid_parquet_file() {
    let path = scratch_path("sample.parquet");
    create_sample_parquet(&path);

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Parquet check for"));
    assert!(stdout.contains("status: VALID"));
    assert!(stdout.contains("rows: 3"));
    assert!(stdout.contains("columns: 3"));
    assert!(stdout.contains("name"));
    assert!(stdout.contains("age"));
    assert!(stdout.contains("active"));

    fs::remove_file(&path).ok();
}

#[test]
fn check_infers_parquet_from_pq_extension() {
    let path = scratch_path("sample.pq");
    create_sample_parquet(&path);

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status: VALID"));
    assert!(stdout.contains("rows: 3"));

    fs::remove_file(&path).ok();
}

#[test]
fn check_fails_on_corrupted_magic_bytes() {
    let path = scratch_path("corrupt_magic.parquet");
    fs::write(&path, b"NOT_PARQUET_FILE_DATA_HERE_1234567890").unwrap();

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("INVALID") || combined.contains("magic"),
        "expected output to indicate invalid parquet/magic bytes, got: {combined}"
    );

    fs::remove_file(&path).ok();
}

#[test]
fn check_fails_on_truncated_parquet_file() {
    let path = scratch_path("truncated.parquet");
    create_sample_parquet(&path);

    // Truncate the file to just header
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, &bytes[..12]).unwrap();

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("INVALID") || combined.contains("failed") || combined.contains("magic"),
        "expected failure message, got: {combined}"
    );

    fs::remove_file(&path).ok();
}

#[test]
fn check_reports_null_counts_and_empty_column_warning() {
    let path = scratch_path("nulls.parquet");
    let mut df = df!(
        "name" => &["Alice", "Bob"],
        "empty_col" => &[Option::<i32>::None, None]
    )
    .unwrap();
    let file = fs::File::create(&path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status: VALID"));
    assert!(stdout.contains("empty_col"));
    assert!(stdout.contains("100.0%") || stdout.contains("empty") || stdout.contains("2 null"));

    fs::remove_file(&path).ok();
}

#[test]
fn check_rejects_unsupported_or_non_parquet_file() {
    let path = scratch_path("sample.csv");
    fs::write(&path, "a,b\n1,2\n").unwrap();

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("parquet") || stderr.contains("format"),
        "stderr: {stderr}"
    );

    fs::remove_file(&path).ok();
}

#[test]
fn check_fails_on_missing_file() {
    let path = scratch_path("does_not_exist.parquet");

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to open") || stderr.contains("No such file"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_with_explicit_format_flag() {
    let path = scratch_path("sample.custom_ext");
    create_sample_parquet(&path);

    let out = bin()
        .arg("check")
        .arg(&path)
        .arg("--format")
        .arg("parquet")
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status: VALID"));

    fs::remove_file(&path).ok();
}

#[test]
fn check_warns_on_unnamed_or_empty_column_name() {
    let path = scratch_path("unnamed_col.parquet");
    let mut df = df!(
        "" => &["1", "2"],
        "val" => &[10, 20]
    )
    .unwrap();
    let file = fs::File::create(&path).unwrap();
    ParquetWriter::new(file).finish(&mut df).unwrap();

    let out = bin()
        .arg("check")
        .arg(&path)
        .output()
        .expect("failed to run csvsum");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("status: VALID"));
    assert!(stdout.contains("(unnamed)") || stdout.contains("empty column name"));

    fs::remove_file(&path).ok();
}
