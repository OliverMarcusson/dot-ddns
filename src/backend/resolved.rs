use crate::error::{DotDdnsError, Result};
use crate::links::ManagedLink;
use tokio::process::Command;

pub async fn discover_links() -> Result<Vec<ManagedLink>> {
    let output = Command::new("ip")
        .args(["-o", "link", "show"])
        .output()
        .await
        .map_err(|e| DotDdnsError::Backend(format!("failed to execute ip: {e}")))?;
    if !output.status.success() {
        return Err(DotDdnsError::Backend(format!(
            "ip link show failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let mut links = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.splitn(3, ':');
        let ifindex = parts
            .next()
            .unwrap_or_default()
            .trim()
            .parse::<u32>()
            .unwrap_or_default();
        let ifname = parts
            .next()
            .unwrap_or_default()
            .trim()
            .trim_end_matches('@')
            .to_string();
        if ifindex == 0 || ifname.is_empty() || ifname == "lo" {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.contains("state down") {
            continue;
        }
        links.push(ManagedLink {
            ifindex,
            ifname,
            source: "resolved".to_string(),
            connection_id: None,
            connection_uuid: None,
            device_type: None,
        });
    }
    Ok(links)
}
