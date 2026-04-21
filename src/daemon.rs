use crate::backend::detect::{detect_backend, DetectedBackend};
use crate::backend::{networkmanager, resolved};
use crate::config::{default_state_path, Config};
use crate::error::{DotDdnsError, ExitCode, Result};
use crate::links::{diff_links, ManagedLink};
use crate::resolvedctl::{apply_link, revert_link};
use crate::resolver::resolve_provider;
use crate::state::AppState;
use std::path::Path;
use tokio::time::{interval, MissedTickBehavior};

#[derive(Debug)]
pub struct ReconcileResult {
    pub changed: bool,
}

pub async fn reconcile_once(
    config: &Config,
    state_path: &Path,
    dry_run: bool,
) -> Result<ReconcileResult> {
    let mut state = AppState::load_or_default(Some(state_path), Some(&config.domain)).await?;
    state.domain = config.domain.clone();
    state.version = 1;

    let detection = detect_backend(config.backend.clone()).await?;
    let backend = match detection.chosen_backend.as_deref() {
        Some("networkmanager") => DetectedBackend::Networkmanager,
        Some("resolved") => DetectedBackend::Resolved,
        _ => {
            return Err(DotDdnsError::Backend(
                "no supported backend detected".into(),
            ))
        }
    };

    let links = discover_links(backend).await?;
    let resolved = match resolve_provider(config).await {
        Ok(set) => {
            state.last_successful_resolve = Some(chrono::Utc::now());
            set
        }
        Err(err) => {
            tracing::warn!(error=%err, "resolution failed, keeping last known good state");
            return Err(err);
        }
    };

    let link_diff = diff_links(&state.managed_links, &links);
    let endpoints_changed = state.last_endpoints != resolved.endpoints;
    let links_changed = !link_diff.added.is_empty() || !link_diff.removed.is_empty();
    let mut changed = false;

    if dry_run {
        tracing::info!(?backend, links = links.len(), endpoints = ?resolved.endpoints, "dry-run reconciliation");
        state.backend = Some(backend.as_str().to_string());
        state.last_ips_v4 = resolved.ipv4;
        state.last_ips_v6 = resolved.ipv6;
        state.last_endpoints = resolved.endpoints;
        state.managed_links = links;
        return Ok(ReconcileResult {
            changed: endpoints_changed || links_changed,
        });
    }

    for link in &link_diff.removed {
        if let Err(err) = revert_link(link).await {
            tracing::warn!(link=%link.short(), error=%err, "failed to revert removed link");
        } else {
            changed = true;
        }
    }

    let target_links = if endpoints_changed {
        &links
    } else {
        &link_diff.added
    };
    if endpoints_changed || !target_links.is_empty() {
        let mut successful = Vec::new();
        let mut errors = Vec::new();
        for link in target_links {
            match apply_link(link, &resolved.endpoints).await {
                Ok(_) => {
                    successful.push(link.clone());
                    changed = true;
                }
                Err(err) => {
                    errors.push(format!("{}: {err}", link.short()));
                }
            }
        }
        if !errors.is_empty() && successful.is_empty() {
            return Err(DotDdnsError::Apply(errors.join("; ")));
        }
        if !errors.is_empty() {
            tracing::warn!(errors = ?errors, "partial link apply failure");
        }
    }

    state.backend = Some(backend.as_str().to_string());
    state.last_ips_v4 = resolved.ipv4;
    state.last_ips_v6 = resolved.ipv6;
    state.last_endpoints = resolved.endpoints;
    state.managed_links = links;
    state.last_apply = Some(chrono::Utc::now());
    state.enabled = true;
    state.save(Some(state_path)).await?;

    Ok(ReconcileResult { changed })
}

pub async fn disable_runtime(config: &Config, state_path: &Path) -> Result<bool> {
    let mut state = AppState::load_or_default(Some(state_path), Some(&config.domain)).await?;
    let mut changed = false;
    for link in &state.managed_links {
        match revert_link(link).await {
            Ok(_) => changed = true,
            Err(err) => {
                tracing::warn!(link=%link.short(), error=%err, "failed to revert link during disable")
            }
        }
    }
    state.enabled = false;
    state.managed_links.clear();
    state.save(Some(state_path)).await?;
    Ok(changed)
}

pub async fn run_daemon(config_path: &Path) -> Result<()> {
    let config = Config::load(config_path).await?;
    let state_path = default_state_path();

    let mut ticker = interval(config.poll_duration()?);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut hup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|e| DotDdnsError::Apply(format!("failed to register SIGHUP handler: {e}")))?;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| DotDdnsError::Apply(format!("failed to register SIGTERM handler: {e}")))?;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let _ = reconcile_once(&config, &state_path, false).await;
            }
            _ = hup.recv() => {
                tracing::info!("received SIGHUP, reconciling now");
                let _ = reconcile_once(&config, &state_path, false).await;
            }
            _ = term.recv() => {
                tracing::info!("received SIGTERM, shutting down");
                break;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received Ctrl-C, shutting down");
                break;
            }
        }
    }

    let _ = disable_runtime(&config, &state_path).await;
    Ok(())
}

pub fn exit_code_for_change(changed: bool) -> ExitCode {
    if changed {
        ExitCode::Changed
    } else {
        ExitCode::Ok
    }
}

async fn discover_links(backend: DetectedBackend) -> Result<Vec<ManagedLink>> {
    let mut links = match backend {
        DetectedBackend::Networkmanager => networkmanager::discover_links().await?,
        DetectedBackend::Resolved => resolved::discover_links().await?,
    };
    links.sort();
    Ok(links)
}
