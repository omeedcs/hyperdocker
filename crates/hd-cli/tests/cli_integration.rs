use std::process::Command;

#[test]
fn hd_help_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_hd"))
        .arg("--help")
        .output()
        .expect("failed to run hd");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hyperdocker"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("up"));
}

#[test]
fn hd_init_creates_toml() {
    let dir = tempfile::TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hd"))
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("failed to run hd init");
    assert!(output.status.success());
    assert!(dir.path().join("hd.toml").exists());

    let content = std::fs::read_to_string(dir.path().join("hd.toml")).unwrap();
    assert!(content.contains("[environment]"));
}

#[test]
fn hd_status_without_toml_fails() {
    let dir = tempfile::TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hd"))
        .arg("status")
        .current_dir(dir.path())
        .output()
        .expect("failed to run hd status");
    assert!(!output.status.success());
}
