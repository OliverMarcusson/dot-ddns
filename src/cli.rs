use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "dot-ddns",
    version,
    about = "Dynamic DNS-over-TLS updater for systemd-resolved"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init(InitArgs),
    Enable(ConfigArgs),
    Disable(ConfigArgs),
    Apply(ApplyArgs),
    Daemon(DaemonArgs),
    Status(StatusArgs),
    DetectBackend(DetectBackendArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub domain: String,
    #[arg(long)]
    pub bootstrap: String,
    #[arg(long, default_value = "2s")]
    pub poll_interval: String,
    #[arg(long, value_enum, default_value = "auto")]
    pub backend: BackendArg,
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
    #[arg(long)]
    pub force: bool,
    #[arg(long, value_enum, default_value = "both")]
    pub ip_family: IpFamilyArg,
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
    #[arg(long)]
    pub runtime_only: bool,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DetectBackendArgs {
    #[arg(long, default_value = "/etc/dot-ddns/config.toml")]
    pub config: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BackendArg {
    Auto,
    Networkmanager,
    Resolved,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum IpFamilyArg {
    Ipv4,
    Ipv6,
    Both,
}
