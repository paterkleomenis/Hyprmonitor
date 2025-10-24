use std::process::{Command, Stdio};

pub fn execute_hyprctl(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn fetch_monitors() -> std::io::Result<Vec<serde_json::Value>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "all", "-j"])
        .output()?;

    serde_json::from_slice(&output.stdout)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
