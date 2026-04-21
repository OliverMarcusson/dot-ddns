use crate::config::default_state_path;
use crate::links::ManagedLink;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::error::{DotDdnsError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub version: u32,
    pub domain: String,
    pub backend: Option<String>,
    pub last_ips_v4: Vec<String>,
    pub last_ips_v6: Vec<String>,
    pub last_endpoints: Vec<String>,
    pub managed_links: Vec<ManagedLink>,
    pub enabled: bool,
    pub last_successful_resolve: Option<chrono::DateTime<chrono::Utc>>,
    pub last_apply: Option<chrono::DateTime<chrono::Utc>>,
}

impl AppState {
    pub fn new(domain: String) -> Self {
        Self {
            version: 1,
            domain,
            ..Default::default()
        }
    }

    pub async fn load_or_default(path: Option<&Path>, domain: Option<&str>) -> Result<Self> {
        let owned_default;
        let path = match path {
            Some(path) => path,
            None => {
                owned_default = default_state_path();
                owned_default.as_path()
            }
        };
        match fs::read_to_string(path).await {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(state) => Ok(state),
                Err(err) => {
                    tracing::warn!(%err, path=%path.display(), "state file corrupt, rebuilding");
                    Ok(Self::new(domain.unwrap_or_default().to_string()))
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::new(domain.unwrap_or_default().to_string()))
            }
            Err(err) => Err(DotDdnsError::StateIo(format!(
                "failed to read state {}: {err}",
                path.display()
            ))),
        }
    }

    pub async fn save(&self, path: Option<&Path>) -> Result<()> {
        let owned_path = path.map(PathBuf::from).unwrap_or_else(default_state_path);
        if let Some(parent) = owned_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                DotDdnsError::StateIo(format!("failed to create {}: {e}", parent.display()))
            })?;
        }
        let temp = owned_path.with_extension(format!("json.tmp.{}", std::process::id()));
        let body = serde_json::to_vec_pretty(self)
            .map_err(|e| DotDdnsError::StateIo(format!("failed to serialize state: {e}")))?;
        fs::write(&temp, body).await.map_err(|e| {
            DotDdnsError::StateIo(format!(
                "failed to write temp state {}: {e}",
                temp.display()
            ))
        })?;
        fs::rename(&temp, &owned_path).await.map_err(|e| {
            DotDdnsError::StateIo(format!(
                "failed to atomically replace state {}: {e}",
                owned_path.display()
            ))
        })?;
        Ok(())
    }
}
