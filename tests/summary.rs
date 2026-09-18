use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_csvsum"))
}

#[test]
fn summarizes_sample_file() {
    let output = bin()
        .arg("examples/sample.csv")
        .output()
        .expect("failed to run csvsum");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Rows:    10"));
    assert!(stdout.contains("Columns: 6"));
    assert!(stdout.contains("age (numeric)"));
    assert!(stdout.contains("name (text)"));
    // age has 2 missing values (Carol, Heidi)
    assert!(stdout.contains("missing: 2"));
}

#[test]
fn errors_on_missing_file() {
    let output = bin()
        .arg("does-not-exist.csv")
        .output()
        .expect("failed to run csvsum");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does-not-exist.csv"));
}
