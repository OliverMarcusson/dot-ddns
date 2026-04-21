use crate::error::{DotDdnsError, Result};
use crate::links::ManagedLink;
use tokio::fs;
use tokio::process::Command;

pub async fn discover_links() -> Result<Vec<ManagedLink>> {
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "DEVICE,TYPE,STATE,CONNECTION",
            "device",
            "status",
        ])
        .output()
        .await
        .map_err(|e| DotDdnsError::Backend(format!("failed to execute nmcli: {e}")))?;
    if !output.status.success() {
        return Err(DotDdnsError::Backend(format!(
            "nmcli device status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let mut links = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<_> = line.split(':').collect();
        if parts.len() < 4 {
            continue;
        }
        let ifname = parts[0].trim();
        let dev_type = parts[1].trim();
        let state = parts[2].trim().to_lowercase();
        let connection_id = parts[3].trim();
        if ifname.is_empty() || ifname == "lo" || state.contains("disconnected") {
            continue;
        }
        let ifindex = read_ifindex(ifname).await?;
        let connection_uuid = connection_uuid(connection_id).await.ok();
        links.push(ManagedLink {
            ifindex,
            ifname: ifname.to_string(),
            source: "networkmanager".to_string(),
            connection_id: (!connection_id.is_empty() && connection_id != "--")
                .then(|| connection_id.to_string()),
            connection_uuid,
            device_type: (!dev_type.is_empty()).then(|| dev_type.to_string()),
        });
    }
    Ok(links)
}

async fn read_ifindex(ifname: &str) -> Result<u32> {
    let path = format!("/sys/class/net/{ifname}/ifindex");
    let content = fs::read_to_string(&path)
        .await
        .map_err(|e| DotDdnsError::Backend(format!("failed to read {path}: {e}")))?;
    content
        .trim()
        .parse()
        .map_err(|e| DotDdnsError::Backend(format!("invalid ifindex in {path}: {e}")))
}

async fn connection_uuid(connection_id: &str) -> Result<String> {
    if connection_id.is_empty() || connection_id == "--" {
        return Err(DotDdnsError::Backend("no connection id".into()));
    }
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-g",
            "connection.uuid",
            "connection",
            "show",
            connection_id,
        ])
        .output()
        .await
        .map_err(|e| DotDdnsError::Backend(format!("failed to execute nmcli: {e}")))?;
    if !output.status.success() {
        return Err(DotDdnsError::Backend(format!(
            "nmcli connection show failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
