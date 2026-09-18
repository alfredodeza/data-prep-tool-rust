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
        "csvsum-schema-test-{}-{}-{}.{}",
        std::process::id(),
        stem,
        nanos,
        ext
    ));
    p
}

#[test]
fn schema_infer_writes_default_output_and_prints_types() {
    let input = scratch_path("infer-input.csv");
    fs::write(
        &input,
        "id,name,rating\n1,Alice,88.5\n2,Bob,92.1\n3,Carol,\n",
    )
    .unwrap();

    let expected_output = {
        let stem = input.file_stem().unwrap().to_string_lossy().to_string();
        input.with_file_name(format!("{stem}.schema.json"))
    };

    let out = bin()
        .arg("schema")
        .arg("infer")
        .arg(&input)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("id"));
    assert!(stdout.contains("integer"));
    assert!(stdout.contains("rating"));
    assert!(stdout.contains("float"));
    assert!(stdout.contains("nullable"));
    assert!(stdout.contains(&expected_output.display().to_string()));

    assert!(expected_output.exists(), "expected schema file to be saved");
    let saved = fs::read_to_string(&expected_output).unwrap();
    assert!(saved.contains("\"id\""));
    assert!(saved.contains("\"integer\""));
    assert!(saved.contains("\"rating\""));
    assert!(saved.contains("\"float\""));

    fs::remove_file(&input).ok();
    fs::remove_file(&expected_output).ok();
}

#[test]
fn schema_infer_custom_output_path() {
    let input = scratch_path("infer-custom-input.csv");
    let schema_path = scratch_path("infer-custom.schema.json");
    fs::write(&input, "a,b\n1,x\n2,y\n").unwrap();

    let out = bin()
        .arg("schema")
        .arg("infer")
        .arg(&input)
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(schema_path.exists());

    fs::remove_file(&input).ok();
    fs::remove_file(&schema_path).ok();
}

#[test]
fn schema_check_reports_compliant() {
    let input = scratch_path("check-ok-input.csv");
    let schema_path = scratch_path("check-ok.schema.json");
    fs::write(&input, "id,name\n1,Alice\n2,Bob\n").unwrap();

    let infer = bin()
        .arg("schema")
        .arg("infer")
        .arg(&input)
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");
    assert!(infer.status.success());

    let check = bin()
        .arg("schema")
        .arg("check")
        .arg(&input)
        .arg("--schema")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");

    assert!(
        check.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("COMPLIANT"));
    assert!(!stdout.contains("NOT COMPLIANT"));

    fs::remove_file(&input).ok();
    fs::remove_file(&schema_path).ok();
}

#[test]
fn schema_check_reports_missing_and_extra_columns() {
    let base_input = scratch_path("check-base-input.csv");
    let schema_path = scratch_path("check-diff.schema.json");
    fs::write(&base_input, "id,name,age\n1,Alice,30\n2,Bob,25\n").unwrap();

    let infer = bin()
        .arg("schema")
        .arg("infer")
        .arg(&base_input)
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");
    assert!(infer.status.success());

    // New file drops "age" and adds "city" compared to the schema.
    let changed_input = scratch_path("check-changed-input.csv");
    fs::write(
        &changed_input,
        "id,name,city\n1,Alice,Seattle\n2,Bob,Austin\n",
    )
    .unwrap();

    let check = bin()
        .arg("schema")
        .arg("check")
        .arg(&changed_input)
        .arg("--schema")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("NOT COMPLIANT"));
    assert!(stdout.contains("missing columns"));
    assert!(stdout.contains("age"));
    assert!(stdout.contains("extra columns"));
    assert!(stdout.contains("city"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&schema_path).ok();
}

#[test]
fn schema_check_reports_type_mismatch() {
    let base_input = scratch_path("check-type-base-input.csv");
    let schema_path = scratch_path("check-type.schema.json");
    fs::write(&base_input, "id,amount\n1,10.5\n2,20.25\n").unwrap();

    let infer = bin()
        .arg("schema")
        .arg("infer")
        .arg(&base_input)
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");
    assert!(infer.status.success());

    // "amount" is text here instead of the expected float.
    let changed_input = scratch_path("check-type-changed-input.csv");
    fs::write(&changed_input, "id,amount\n1,ten-fifty\n2,twenty\n").unwrap();

    let check = bin()
        .arg("schema")
        .arg("check")
        .arg(&changed_input)
        .arg("--schema")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("NOT COMPLIANT"));
    assert!(stdout.contains("type mismatch"));
    assert!(stdout.contains("amount"));
    assert!(stdout.contains("expected float"));
    assert!(stdout.contains("found text"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&schema_path).ok();
}

#[test]
fn schema_check_reports_nullability_violation() {
    let base_input = scratch_path("check-null-base-input.csv");
    let schema_path = scratch_path("check-null.schema.json");
    fs::write(&base_input, "id,name\n1,Alice\n2,Bob\n").unwrap();

    let infer = bin()
        .arg("schema")
        .arg("infer")
        .arg(&base_input)
        .arg("--output")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");
    assert!(infer.status.success());

    // "name" now has a missing value, violating the not-null schema.
    let changed_input = scratch_path("check-null-changed-input.csv");
    fs::write(&changed_input, "id,name\n1,Alice\n2,\n").unwrap();

    let check = bin()
        .arg("schema")
        .arg("check")
        .arg(&changed_input)
        .arg("--schema")
        .arg(&schema_path)
        .output()
        .expect("failed to run csvsum");

    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("NOT COMPLIANT"));
    assert!(stdout.contains("nullability violation"));
    assert!(stdout.contains("name"));

    fs::remove_file(&base_input).ok();
    fs::remove_file(&changed_input).ok();
    fs::remove_file(&schema_path).ok();
}

#[test]
fn schema_check_errors_on_missing_schema_file() {
    let out = bin()
        .arg("schema")
        .arg("check")
        .arg("examples/sample.csv")
        .arg("--schema")
        .arg("does-not-exist.schema.json")
        .output()
        .expect("failed to run csvsum");

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("does-not-exist.schema.json"));
}
