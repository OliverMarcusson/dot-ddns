use crate::error::{DotDdnsError, Result};
use tokio::process::Command;

pub async fn is_service_active(unit: &str) -> Result<bool> {
    let status = Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .await
        .map_err(|e| DotDdnsError::Backend(format!("failed to run systemctl: {e}")))?;
    Ok(status.success())
}

pub async fn enable_now(unit: &str) -> Result<()> {
    run_systemctl(&["enable", "--now", unit]).await
}

pub async fn disable_now(unit: &str) -> Result<()> {
    run_systemctl(&["disable", "--now", unit]).await
}

async fn run_systemctl(args: &[&str]) -> Result<()> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .await
        .map_err(|e| DotDdnsError::Apply(format!("failed to execute systemctl: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(DotDdnsError::Apply(format!(
        "systemctl {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}
