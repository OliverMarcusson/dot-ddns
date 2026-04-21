use crate::config::BackendPreference;
use crate::error::{DotDdnsError, Result};
use crate::systemd::is_service_active;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectedBackend {
    Networkmanager,
    Resolved,
}

impl DetectedBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Networkmanager => "networkmanager",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DetectionReport {
    pub networkmanager_active: bool,
    pub resolved_active: bool,
    pub chosen_backend: Option<String>,
    pub failure_reason: Option<String>,
}

pub async fn detect_backend(preference: BackendPreference) -> Result<DetectionReport> {
    let resolved_active = is_service_active("systemd-resolved.service").await?;
    let networkmanager_active = is_service_active("NetworkManager.service").await?;

    let outcome = match preference {
        BackendPreference::Networkmanager => {
            if !networkmanager_active {
                Err("NetworkManager requested but service is not active".to_string())
            } else if !resolved_active {
                Err("systemd-resolved is not active; dot-ddns requires systemd-resolved for runtime DoT application".to_string())
            } else {
                Ok(DetectedBackend::Networkmanager)
            }
        }
        BackendPreference::Resolved => {
            if !resolved_active {
                Err("systemd-resolved is not active; dot-ddns requires systemd-resolved for runtime DoT application".to_string())
            } else {
                Ok(DetectedBackend::Resolved)
            }
        }
        BackendPreference::Auto => {
            if !resolved_active {
                Err("systemd-resolved is not active; dot-ddns requires systemd-resolved for runtime DoT application".to_string())
            } else if networkmanager_active {
                Ok(DetectedBackend::Networkmanager)
            } else if resolved_active {
                Ok(DetectedBackend::Resolved)
            } else {
                Err("no supported backend detected".to_string())
            }
        }
    };

    match outcome {
        Ok(backend) => Ok(DetectionReport {
            networkmanager_active,
            resolved_active,
            chosen_backend: Some(backend.as_str().to_string()),
            failure_reason: None,
        }),
        Err(reason) => Err(DotDdnsError::Backend(reason)),
    }
}
