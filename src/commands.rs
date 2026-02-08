use std::io;
use std::process::{Command, Stdio};

pub fn execute_hyprctl_args(args: &[&str]) -> bool {
    Command::new("hyprctl")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn fetch_monitors() -> io::Result<Vec<serde_json::Value>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "all", "-j"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("hyprctl monitors failed: {}", stderr.trim()),
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
