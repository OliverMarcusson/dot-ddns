use crate::error::{DotDdnsError, Result};
use crate::links::ManagedLink;
use tokio::process::Command;

async fn run_resolvectl(args: &[String]) -> Result<()> {
    let output = Command::new("resolvectl")
        .args(args)
        .output()
        .await
        .map_err(|e| DotDdnsError::Apply(format!("failed to execute resolvectl: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(DotDdnsError::Apply(format!(
        "resolvectl {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

pub async fn apply_link(link: &ManagedLink, endpoints: &[String]) -> Result<()> {
    tracing::info!(link = %link.short(), ?endpoints, "applying runtime DoT configuration");
    let mut dns_args = vec!["dns".to_string(), link.ifname.clone()];
    dns_args.extend(endpoints.iter().cloned());
    run_resolvectl(&dns_args).await?;
    run_resolvectl(&[
        "dnsovertls".to_string(),
        link.ifname.clone(),
        "yes".to_string(),
    ])
    .await?;
    run_resolvectl(&["domain".to_string(), link.ifname.clone(), "~.".to_string()]).await?;
    run_resolvectl(&[
        "default-route".to_string(),
        link.ifname.clone(),
        "yes".to_string(),
    ])
    .await?;
    Ok(())
}

pub async fn revert_link(link: &ManagedLink) -> Result<()> {
    tracing::info!(link = %link.short(), "reverting runtime DoT configuration");
    run_resolvectl(&["revert".to_string(), link.ifname.clone()]).await
}
