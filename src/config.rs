use crate::cli::{BackendArg, InitArgs, IpFamilyArg};
use crate::error::{DotDdnsError, Result};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs;

pub const DEFAULT_STATE_PATH: &str = "/var/lib/dot-ddns/state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BackendPreference {
    Auto,
    Networkmanager,
    Resolved,
}

impl BackendPreference {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Networkmanager => "networkmanager",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IpFamily {
    Ipv4,
    Ipv6,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub domain: String,
    pub bootstrap: Vec<String>,
    pub poll_interval: String,
    pub backend: BackendPreference,
    pub ip_family: IpFamily,
    pub log_level: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapServer {
    pub addr: SocketAddr,
}

impl Display for BootstrapServer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.addr)
    }
}

impl BootstrapServer {}

impl FromStr for BootstrapServer {
    type Err = DotDdnsError;

    fn from_str(value: &str) -> Result<Self> {
        if let Ok(addr) = value.parse::<SocketAddr>() {
            return Ok(Self { addr });
        }

        if let Ok(ip) = value.parse::<IpAddr>() {
            return Ok(Self {
                addr: SocketAddr::new(ip, 53),
            });
        }

        Err(DotDdnsError::Config(format!(
            "invalid bootstrap server '{value}'; expected IP[:port] or [IPv6]:port literal"
        )))
    }
}

impl Config {
    pub fn from_init_args(args: &InitArgs) -> Result<Self> {
        let config = Self {
            domain: args.domain.clone(),
            bootstrap: args
                .bootstrap
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            poll_interval: args.poll_interval.clone(),
            backend: match args.backend {
                BackendArg::Auto => BackendPreference::Auto,
                BackendArg::Networkmanager => BackendPreference::Networkmanager,
                BackendArg::Resolved => BackendPreference::Resolved,
            },
            ip_family: match args.ip_family {
                IpFamilyArg::Ipv4 => IpFamily::Ipv4,
                IpFamilyArg::Ipv6 => IpFamily::Ipv6,
                IpFamilyArg::Both => IpFamily::Both,
            },
            log_level: args.log_level.clone(),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        validate_domain(&self.domain)?;
        if self.bootstrap.is_empty() {
            return Err(DotDdnsError::Config(
                "bootstrap resolver list must not be empty".into(),
            ));
        }
        for item in &self.bootstrap {
            let _ = BootstrapServer::from_str(item)?;
        }
        let duration = parse_duration(&self.poll_interval)?;
        if duration.as_secs() < 1 {
            return Err(DotDdnsError::Config(
                "poll_interval must be at least 1s".into(),
            ));
        }
        Ok(())
    }

    pub fn bootstrap_servers(&self) -> Result<Vec<BootstrapServer>> {
        self.bootstrap
            .iter()
            .map(|entry| BootstrapServer::from_str(entry))
            .collect()
    }

    pub fn poll_duration(&self) -> Result<std::time::Duration> {
        parse_duration(&self.poll_interval)
    }

    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).await.map_err(|e| {
            DotDdnsError::Config(format!("failed to read config {}: {e}", path.display()))
        })?;
        let cfg: Self = toml::from_str(&content).map_err(|e| {
            DotDdnsError::Config(format!("failed to parse config {}: {e}", path.display()))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub async fn save(&self, path: &Path, force: bool) -> Result<()> {
        self.validate()?;
        if path.exists() && !force {
            return Err(DotDdnsError::Config(format!(
                "config {} already exists; use --force to overwrite",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                DotDdnsError::Config(format!("failed to create {}: {e}", parent.display()))
            })?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| DotDdnsError::Config(format!("failed to serialize config: {e}")))?;
        fs::write(path, content).await.map_err(|e| {
            DotDdnsError::Config(format!("failed to write config {}: {e}", path.display()))
        })?;
        Ok(())
    }
}

fn validate_domain(domain: &str) -> Result<()> {
    if domain.parse::<IpAddr>().is_ok() {
        return Err(DotDdnsError::Config(
            "domain must be a hostname, not an IP literal".into(),
        ));
    }
    if domain.is_empty() || domain.len() > 253 {
        return Err(DotDdnsError::Config("invalid domain length".into()));
    }
    if domain.starts_with('.') || domain.ends_with('.') || !domain.contains('.') {
        return Err(DotDdnsError::Config(
            "domain must be a fully qualified hostname".into(),
        ));
    }
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(DotDdnsError::Config(format!(
                "invalid domain label '{label}'"
            )));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            || label.starts_with('-')
            || label.ends_with('-')
        {
            return Err(DotDdnsError::Config(format!(
                "invalid domain label '{label}'"
            )));
        }
    }
    Ok(())
}

pub fn parse_duration(value: &str) -> Result<std::time::Duration> {
    let value = value.trim();
    let (num, unit) = value
        .chars()
        .position(|c| !c.is_ascii_digit())
        .map(|idx| value.split_at(idx))
        .ok_or_else(|| DotDdnsError::Config(format!("invalid duration '{value}'")))?;
    let amount: u64 = num
        .parse()
        .map_err(|_| DotDdnsError::Config(format!("invalid duration '{value}'")))?;
    let duration = match unit {
        "s" => std::time::Duration::from_secs(amount),
        "m" => std::time::Duration::from_secs(amount * 60),
        "h" => std::time::Duration::from_secs(amount * 60 * 60),
        _ => {
            return Err(DotDdnsError::Config(format!(
                "invalid duration unit in '{value}', expected s/m/h"
            )))
        }
    };
    Ok(duration)
}

pub fn default_state_path() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bootstrap() {
        assert!(BootstrapServer::from_str("9.9.9.9").is_ok());
        assert!(BootstrapServer::from_str("9.9.9.9:53").is_ok());
        assert!(BootstrapServer::from_str("[2620:fe::fe]:53").is_ok());
        assert!(BootstrapServer::from_str("dns.example.com:53").is_err());
    }

    #[test]
    fn validates_domain() {
        assert!(validate_domain("one.one.one.one").is_ok());
        assert!(validate_domain("1.1.1.1").is_err());
        assert!(validate_domain("bad_domain").is_err());
    }
}
