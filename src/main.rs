mod backend;
mod cli;
mod config;
mod daemon;
mod error;
mod links;
mod resolvedctl;
mod resolver;
mod state;
mod systemd;

use clap::Parser;
use cli::{Cli, Command};
use config::{default_state_path, Config};
use daemon::{disable_runtime, exit_code_for_change, reconcile_once, run_daemon};
use error::{DotDdnsError, ExitCode};
use serde::Serialize;
use std::path::Path;
use systemd::{disable_now, enable_now, is_service_active};

#[derive(Serialize)]
struct StatusOutput {
    config: String,
    enabled: bool,
    domain: Option<String>,
    backend_configured: Option<String>,
    backend_detected: Option<String>,
    poll_interval: Option<String>,
    bootstrap: Vec<String>,
    current_endpoints: Vec<String>,
    managed_links: Vec<String>,
    last_successful_resolve: Option<chrono::DateTime<chrono::Utc>>,
    last_apply: Option<chrono::DateTime<chrono::Utc>>,
    runtime_dot_active: bool,
    state_file: String,
    service_active: bool,
}

#[tokio::main]
async fn main() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
    let cli = Cli::parse();
    let code = match run(cli).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            err.exit_code()
        }
    };
    std::process::exit(code.as_i32());
}

async fn run(cli: Cli) -> error::Result<ExitCode> {
    match cli.command {
        Command::Init(args) => {
            ensure_root_for_path(&args.config)?;
            let config = Config::from_init_args(&args)?;
            config.save(&args.config, args.force).await?;
            println!("wrote {}", args.config.display());
            Ok(ExitCode::Changed)
        }
        Command::Enable(args) => {
            ensure_root()?;
            let config = Config::load(&args.config).await?;
            let state_path = default_state_path();
            let result = reconcile_once(&config, &state_path, false).await?;
            if !args.runtime_only {
                enable_now("dot-ddns.service").await?;
            }
            Ok(exit_code_for_change(result.changed))
        }
        Command::Disable(args) => {
            ensure_root()?;
            let config = Config::load(&args.config).await?;
            let state_path = default_state_path();
            let changed = disable_runtime(&config, &state_path).await?;
            if !args.runtime_only {
                disable_now("dot-ddns.service").await?;
            }
            Ok(exit_code_for_change(changed))
        }
        Command::Apply(args) => {
            if !args.dry_run {
                ensure_root()?;
            }
            let config = Config::load(&args.config).await?;
            let state_path = default_state_path();
            let result = reconcile_once(&config, &state_path, args.dry_run).await?;
            Ok(exit_code_for_change(result.changed))
        }
        Command::Daemon(args) => {
            ensure_root()?;
            run_daemon(&args.config).await?;
            Ok(ExitCode::Ok)
        }
        Command::Status(args) => {
            let config = Config::load(&args.config).await.ok();
            let state_path = default_state_path();
            let state = state::AppState::load_or_default(Some(&state_path), None).await?;
            let backend_detected = if let Some(cfg) = &config {
                backend::detect::detect_backend(cfg.backend.clone())
                    .await
                    .ok()
                    .and_then(|r| r.chosen_backend)
            } else {
                None
            };
            let service_active = is_service_active("dot-ddns.service").await.unwrap_or(false);
            let status = StatusOutput {
                config: args.config.display().to_string(),
                enabled: state.enabled,
                domain: config
                    .as_ref()
                    .map(|c| c.domain.clone())
                    .or_else(|| (!state.domain.is_empty()).then(|| state.domain.clone())),
                backend_configured: config.as_ref().map(|c| c.backend.as_str().to_string()),
                backend_detected,
                poll_interval: config.as_ref().map(|c| c.poll_interval.clone()),
                bootstrap: config
                    .as_ref()
                    .map(|c| c.bootstrap.clone())
                    .unwrap_or_default(),
                current_endpoints: state.last_endpoints.clone(),
                managed_links: state.managed_links.iter().map(|l| l.short()).collect(),
                last_successful_resolve: state.last_successful_resolve,
                last_apply: state.last_apply,
                runtime_dot_active: state.enabled
                    && !state.managed_links.is_empty()
                    && !state.last_endpoints.is_empty(),
                state_file: default_state_path().display().to_string(),
                service_active,
            };
            if args.json {
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
            } else {
                print_status_human(&status);
            }
            Ok(ExitCode::Ok)
        }
        Command::DetectBackend(args) => {
            let preference = Config::load(&args.config)
                .await
                .map(|c| c.backend)
                .unwrap_or(config::BackendPreference::Auto);
            let report = backend::detect::detect_backend(preference).await?;
            println!("is NetworkManager active: {}", report.networkmanager_active);
            println!("is systemd-resolved active: {}", report.resolved_active);
            if let Some(chosen) = report.chosen_backend {
                println!("chosen backend: {chosen}");
            }
            if let Some(reason) = report.failure_reason {
                println!("failure reason: {reason}");
            }
            Ok(ExitCode::Ok)
        }
    }
}

fn print_status_human(status: &StatusOutput) {
    println!("config: {}", status.config);
    println!("enabled: {}", status.enabled);
    if let Some(domain) = &status.domain {
        println!("domain: {domain}");
    }
    if let Some(backend) = &status.backend_configured {
        println!("backend configured: {backend}");
    }
    if let Some(backend) = &status.backend_detected {
        println!("backend detected: {backend}");
    }
    if let Some(interval) = &status.poll_interval {
        println!("poll interval: {interval}");
    }
    println!("bootstrap resolvers:");
    for resolver in &status.bootstrap {
        println!("  - {resolver}");
    }
    println!("current endpoints:");
    for endpoint in &status.current_endpoints {
        println!("  - {endpoint}");
    }
    println!("managed links:");
    for link in &status.managed_links {
        println!("  - {link}");
    }
    if let Some(ts) = status.last_successful_resolve {
        println!("last successful resolve: {ts}");
    }
    if let Some(ts) = status.last_apply {
        println!("last apply: {ts}");
    }
    println!("runtime DoT active: {}", status.runtime_dot_active);
    println!("service active: {}", status.service_active);
    println!("state file: {}", status.state_file);
}

fn ensure_root() -> error::Result<()> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(DotDdnsError::Permission(
            "this command requires root privileges".into(),
        ))
    }
}

fn ensure_root_for_path(path: &Path) -> error::Result<()> {
    if path.starts_with("/etc") {
        ensure_root()
    } else {
        Ok(())
    }
}
