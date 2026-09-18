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
        "csvsum-drift-test-{}-{}-{}.{}",
        std::process::id(),
        stem,
        nanos,
        ext
    ));
    p
}

#[test]
fn drift_baseline_writes_default_output_and_prints_ranges() {
    let input = scratch_path("baseline-input.csv");
    fs::write(
        &input,
        "id,score,region\n1,10,west\n2,20,east\n3,30,west\n4,40,north\n",
    )
    .unwrap();

    let expected_output = {
        let stem = input.file_stem().unwrap().to_string_lossy().to_string();
        input.with_file_name(format!("{stem}.drift.json"))
    };

    let out = bin()
        .arg("drift")
        .arg("baseline")
        .arg(&input)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("score"));
    assert!(stdout.contains("min"));
    assert!(stdout.contains("max"));
    assert!(stdout.contains("region"));
    assert!(stdout.contains("categories"));
    assert!(stdout.contains(&expected_output.display().to_string()));

    assert!(expected_output.exists(), "expected drift file to be saved");
    let saved = fs::read_to_string(&expected_output).unwrap();
    assert!(saved.contains("\"score\""));
    assert!(saved.contains("\"region\""));
    assert!(saved.contains("\"west\""));

    fs::remove_file(&input).ok();
    fs::remove_file(&expected_output).ok();
}

#[test]
fn drift_check_reports_no_drift_for_same_data() {
    let input = scratch_path("check-ok-input.csv");
    let baseline_path = scratch_path("check-ok.drift.json");
    fs::write(
        &input,
        "id,score,region\n1,10,west\n2,20,east\n3,30,west\n4,40,north\n",
    )
    .unwrap();

    let base = bin()
        .arg("drift")
        .arg("baseline")
        .arg(&input)
        .arg("--output")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");
    assert!(base.status.success());

    let check = bin()
        .arg("drift")
        .arg("check")
        .arg(&input)
        .arg("--baseline")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");

    assert!(
        check.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("NO DRIFT") || stdout.contains("OK"));

    fs::remove_file(&input).ok();
    fs::remove_file(&baseline_path).ok();
}

#[test]
fn drift_check_fails_on_numeric_range_drift() {
    let base_input = scratch_path("check-range-base-input.csv");
    let baseline_path = scratch_path("check-range.drift.json");
    fs::write(&base_input, "id,score\n1,10\n2,20\n3,30\n4,40\n").unwrap();

    let base = bin()
        .arg("drift")
        .arg("baseline")
        .arg(&base_input)
        .arg("--output")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");
    assert!(base.status.success());

    // New data has a score far outside the baseline's [10, 40] range.
    let changed_input = scratch_path("check-range-changed-input.csv");
    fs::write(&changed_input, "id,score\n1,10\n2,20\n3,999\n4,40\n").unwrap();

    let check = bin()
        .arg("drift")
        .arg("check")
        .arg(&changed_input)
        .arg("--baseline")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("DRIFT DETECTED"));
    assert!(stdout.contains("score"));
    assert!(stdout.contains("max"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&baseline_path).ok();
}

#[test]
fn drift_check_warning_mode_does_not_fail() {
    let base_input = scratch_path("check-warn-base-input.csv");
    let baseline_path = scratch_path("check-warn.drift.json");
    fs::write(&base_input, "id,score\n1,10\n2,20\n3,30\n4,40\n").unwrap();

    let base = bin()
        .arg("drift")
        .arg("baseline")
        .arg(&base_input)
        .arg("--output")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");
    assert!(base.status.success());

    let changed_input = scratch_path("check-warn-changed-input.csv");
    fs::write(&changed_input, "id,score\n1,10\n2,20\n3,999\n4,40\n").unwrap();

    let check = bin()
        .arg("drift")
        .arg("check")
        .arg(&changed_input)
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--mode")
        .arg("warn")
        .output()
        .expect("failed to run csvsum");

    assert!(
        check.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("WARNING"));
    assert!(stdout.contains("score"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&baseline_path).ok();
}

#[test]
fn drift_check_detects_categorical_cardinality_drift() {
    let base_input = scratch_path("check-cat-base-input.csv");
    let baseline_path = scratch_path("check-cat.drift.json");
    // Baseline has 4 distinct regions.
    fs::write(
        &base_input,
        "id,region\n1,west\n2,east\n3,north\n4,south\n5,west\n6,east\n",
    )
    .unwrap();

    let base = bin()
        .arg("drift")
        .arg("baseline")
        .arg(&base_input)
        .arg("--output")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");
    assert!(base.status.success());

    // New data collapses to a single region: a sharp cardinality drop.
    let changed_input = scratch_path("check-cat-changed-input.csv");
    fs::write(
        &changed_input,
        "id,region\n1,west\n2,west\n3,west\n4,west\n5,west\n6,west\n",
    )
    .unwrap();

    let check = bin()
        .arg("drift")
        .arg("check")
        .arg(&changed_input)
        .arg("--baseline")
        .arg(&baseline_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("DRIFT DETECTED"));
    assert!(stdout.contains("region"));
    assert!(stdout.contains("categor"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&baseline_path).ok();
}

#[test]
fn drift_check_errors_on_missing_baseline_file() {
    let out = bin()
        .arg("drift")
        .arg("check")
        .arg("examples/sample.csv")
        .arg("--baseline")
        .arg("does-not-exist.drift.json")
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does-not-exist.drift.json"));
}
