# dot-ddns

`dot-ddns` is a Rust CLI + daemon that keeps DNS-over-TLS upstream IPs up to date for `systemd-resolved`.

It resolves a provider hostname through independent bootstrap DNS resolvers, then applies the resolved IPs to active links as runtime `systemd-resolved` DNS servers with the provider hostname used as the TLS authentication name.

## Requirements

- Linux with `systemd-resolved`
- `resolvectl`
- either:
  - `NetworkManager` + `systemd-resolved`, or
  - raw `systemd-resolved` without NetworkManager
- root privileges for `init` to `/etc`, `enable`, `disable`, `apply`, and `daemon`

## Features

- strict DNS-over-TLS only
- IPv4 + IPv6 support
- uses all resolved A/AAAA records
- backend auto-detection
- runtime per-link configuration via `resolvectl`
- daemon polling every 2 seconds by default
- `enable`, `disable`, `apply`, `status`, and `detect-backend` commands

## Build

```bash
cargo build --release --locked
```

## Quick start

Create config:

```bash
dot-ddns init \
  --domain one.one.one.one \
  --bootstrap 9.9.9.9,1.1.1.1,[2620:fe::fe],[2606:4700:4700::1111]
```

Apply and enable service:

```bash
sudo dot-ddns enable
```

Inspect status:

```bash
dot-ddns status
```

Disable runtime management and stop service:

```bash
sudo dot-ddns disable
```

## Commands

```text
dot-ddns init
dot-ddns enable
dot-ddns disable
dot-ddns apply
dot-ddns daemon
dot-ddns status
dot-ddns detect-backend
```

## Example config

See `packaging/config.example.toml`.

## How enable/disable works

- `enable` resolves the configured hostname through bootstrap resolvers
- active links are discovered from NetworkManager or the kernel
- DoT runtime settings are applied per link with `resolvectl`
- unless `--runtime-only` is set, `dot-ddns.service` is enabled and started

`disable`:

- reverts runtime per-link settings with `resolvectl revert LINK`
- clears current owned links from state
- preserves config on disk
- unless `--runtime-only` is set, disables and stops the service

## Troubleshooting

### `systemd-resolved is not active`

`dot-ddns` requires `systemd-resolved` for runtime DoT application.

### `NetworkManager requested but service is not active`

Set `backend = "auto"` or `backend = "resolved"`, or start NetworkManager.

### resolution failures

Verify bootstrap resolvers are reachable on port 53 and the provider domain has usable A/AAAA records.

### nothing is applied

Check:

- `dot-ddns detect-backend`
- `dot-ddns status`
- `journalctl -u dot-ddns.service`

## Packaging

Packaging assets live in `packaging/`:

- `dot-ddns.service`
- `dot-ddns.tmpfiles`
- `dot-ddns.install`
- `PKGBUILD`
