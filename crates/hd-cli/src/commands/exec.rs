pub fn run(cmd: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if cmd.is_empty() {
        return Err("Usage: hd exec <command> [args...]".into());
    }
    // For v1, exec runs the command directly (no sandbox isolation)
    let status = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .status()?;
    std::process::exit(status.code().unwrap_or(1));
}
