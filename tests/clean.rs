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

#[test]
fn clean_drops_fully_empty_column() {
    let input = scratch_path("empty-col-input.csv");
    let output = scratch_path("empty-col-output.csv");
    fs::write(
        &input,
        "id,name,grape,rating\n1,Alice,,90\n2,Bob,,85\n3,Carol,,88\n",
    )
    .unwrap();

    let out = bin()
        .arg("--clean")
        .arg("--output")
        .arg(&output)
        .arg(&input)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("columns dropped:  1 (grape)"));
    assert!(stdout.contains("columns kept:     3"));
    assert!(stdout.contains("rows with nulls:  0"));

    let cleaned = fs::read_to_string(&output).unwrap();
    let header = cleaned.lines().next().unwrap();
    assert_eq!(header, "id,name,rating");
    assert!(!cleaned.contains("grape"));

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
}

#[test]
fn clean_warns_and_counts_rows_with_missing_fields() {
    let input = scratch_path("missing-fields-input.csv");
    let output = scratch_path("missing-fields-output.csv");
    // "city" has one blank (row 2 / Bob), "age" is fully populated.
    fs::write(
        &input,
        "id,name,age,city\n1,Alice,30,Seattle\n2,Bob,25,\n3,Carol,,Austin\n",
    )
    .unwrap();

    let out = bin()
        .arg("--clean")
        .arg("--output")
        .arg(&output)
        .arg(&input)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Row numbers count the header as row 1, so data rows are 2 and 3.
    assert!(stderr.contains("WARNING:"));
    assert!(stderr.contains("row 3: missing value in column(s): city"));
    assert!(stderr.contains("row 4: missing value in column(s): age"));

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("columns dropped:  0"));
    assert!(stdout.contains("rows with nulls:  2 (66.7%)"));
    assert!(stdout.contains("rows processed:   3"));

    // No columns were fully empty, so all 4 remain in the cleaned file.
    let cleaned = fs::read_to_string(&output).unwrap();
    assert_eq!(cleaned.lines().next().unwrap(), "id,name,age,city");

    fs::remove_file(&input).ok();
    fs::remove_file(&output).ok();
}

#[test]
fn clean_default_output_path_next_to_input() {
    let input = scratch_path("default-output-input.csv");
    fs::write(&input, "a,b,empty\n1,2,\n3,4,\n").unwrap();

    let out = bin()
        .arg("--clean")
        .arg(&input)
        .output()
        .expect("failed to run csvsum");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let expected_output = {
        let stem = input.file_stem().unwrap().to_string_lossy().to_string();
        input.with_file_name(format!("{stem}.clean.csv"))
    };
    assert!(
        expected_output.exists(),
        "expected {:?} to exist",
        expected_output
    );
    let cleaned = fs::read_to_string(&expected_output).unwrap();
    assert_eq!(cleaned.lines().next().unwrap(), "a,b");

    fs::remove_file(&input).ok();
    fs::remove_file(&expected_output).ok();
}

#[test]
fn clean_rejects_output_flag_with_multiple_files() {
    let out = bin()
        .arg("--clean")
        .arg("--output")
        .arg("wont-be-created.csv")
        .arg("examples/sample.csv")
        .arg("examples/wine-ratings.csv")
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--output can only be used with a single input file"));
}
