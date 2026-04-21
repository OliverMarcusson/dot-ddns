use crate::config::{BootstrapServer, Config, IpFamily};
use crate::error::{DotDdnsError, Result};
use hickory_proto::xfer::Protocol;
use hickory_resolver::config::{NameServerConfig, NameServerConfigGroup, ResolverConfig};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::Resolver;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedSet {
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub endpoints: Vec<String>,
}

pub async fn resolve_provider(config: &Config) -> Result<ResolvedSet> {
    let bootstrap = config.bootstrap_servers()?;
    let resolver = build_resolver(&bootstrap);

    let mut ipv4 = BTreeSet::new();
    let mut ipv6 = BTreeSet::new();

    match config.ip_family {
        IpFamily::Ipv4 | IpFamily::Both => match resolver.ipv4_lookup(config.domain.as_str()).await
        {
            Ok(lookup) => {
                for addr in lookup.iter() {
                    ipv4.insert(addr.to_string());
                }
            }
            Err(err) if err.is_no_records_found() => {
                tracing::debug!(domain = %config.domain, "no A records returned");
            }
            Err(err) => {
                return Err(DotDdnsError::Resolution(format!(
                    "A lookup failed for {}: {err}",
                    config.domain
                )));
            }
        },
        IpFamily::Ipv6 => {}
    }

    match config.ip_family {
        IpFamily::Ipv6 | IpFamily::Both => match resolver.ipv6_lookup(config.domain.as_str()).await
        {
            Ok(lookup) => {
                for addr in lookup.iter() {
                    ipv6.insert(addr.to_string());
                }
            }
            Err(err) if err.is_no_records_found() => {
                tracing::debug!(domain = %config.domain, "no AAAA records returned");
            }
            Err(err) => {
                return Err(DotDdnsError::Resolution(format!(
                    "AAAA lookup failed for {}: {err}",
                    config.domain
                )));
            }
        },
        IpFamily::Ipv4 => {}
    }

    if ipv4.is_empty() && ipv6.is_empty() {
        return Err(DotDdnsError::Resolution(format!(
            "no usable A/AAAA records returned for {}",
            config.domain
        )));
    }

    let ipv4: Vec<String> = ipv4.into_iter().collect();
    let ipv6: Vec<String> = ipv6.into_iter().collect();
    let mut endpoints = Vec::new();
    for ip in &ipv4 {
        endpoints.push(format!("{ip}#{}", config.domain));
    }
    for ip in &ipv6 {
        endpoints.push(format!("[{ip}]#{}", config.domain));
    }
    Ok(ResolvedSet {
        ipv4,
        ipv6,
        endpoints,
    })
}

fn build_resolver(bootstrap: &[BootstrapServer]) -> Resolver<TokioConnectionProvider> {
    let mut group = NameServerConfigGroup::new();
    for server in bootstrap {
        group.push(NameServerConfig {
            socket_addr: server.addr,
            protocol: Protocol::Udp,
            tls_dns_name: None,
            trust_negative_responses: false,
            bind_addr: None,
            http_endpoint: None,
        });
    }
    let resolver_config = ResolverConfig::from_parts(None, vec![], group);
    let mut builder =
        Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default());
    builder.options_mut().timeout = std::time::Duration::from_secs(1);
    builder.options_mut().attempts = 1;
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn endpoint_formatting() {
        let config = Config {
            domain: "one.one.one.one".into(),
            bootstrap: vec!["1.1.1.1:53".into()],
            poll_interval: "2s".into(),
            backend: crate::config::BackendPreference::Auto,
            ip_family: IpFamily::Both,
            log_level: "info".into(),
        };
        let set = ResolvedSet {
            ipv4: vec!["1.0.0.1".into(), "1.1.1.1".into()],
            ipv6: vec!["2606:4700:4700::1111".into()],
            endpoints: vec![
                format!("{}#{}", "1.0.0.1", config.domain),
                format!("{}#{}", "1.1.1.1", config.domain),
                format!("[{}]#{}", "2606:4700:4700::1111", config.domain),
            ],
        };
        assert_eq!(set.endpoints[2], "[2606:4700:4700::1111]#one.one.one.one");
    }

    #[test]
    fn canonical_sort_behavior() {
        let mut ips = BTreeSet::<IpAddr>::new();
        ips.insert("1.1.1.1".parse().unwrap());
        ips.insert("1.0.0.1".parse().unwrap());
        let v: Vec<_> = ips.into_iter().collect();
        assert_eq!(v[0].to_string(), "1.0.0.1");
    }
}
