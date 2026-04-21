# dot-ddns Implementation Specification

Version: v1 draft  
Date: 2026-04-21

## 1. Purpose

`dot-ddns` is a Rust CLI and daemon for systems using `systemd-resolved`, with optional `NetworkManager` integration for link discovery.

Its purpose is to:

1. Resolve a DNS-over-TLS provider hostname to its current A/AAAA addresses using independent bootstrap DNS resolvers.
2. Configure those resolved IPs as the upstream DNS servers for DNS over TLS.
3. Use the same hostname as the TLS authentication/SNI name.
4. Detect whether the system is managed by:
   - `NetworkManager`, or
   - raw `systemd-resolved`
5. Apply configuration to all active links.
6. Continuously refresh the configured DoT IPs within seconds when the provider IP set changes.
7. Allow DoT to be enabled, disabled, reapplied, and inspected from a CLI.
8. Be suitable for Arch Linux packaging and publication on the AUR.

---

## 2. Product decisions

These decisions are fixed for v1.

- Project name: `dot-ddns`
- Binary name: `dot-ddns`
- Package name: `dot-ddns`
- Language: Rust
- Primary platform: Arch Linux
- DNS-over-TLS mode: strict only
- Address families: IPv4 + IPv6 by default
- Use all resolved IPs, not only the first
- Backend detection behavior:
  - prefer `NetworkManager` when active
  - otherwise use `systemd-resolved`
  - otherwise fail
- Update model: long-running daemon, not a timer
- Default refresh target: within a couple seconds
- Default poll interval: `2s`
- Bootstrap resolvers: configurable and required in practice for reliable operation
- Disable/enable behavior:
  - `disable` reverts runtime DoT state and stops daemon behavior
  - config remains on disk
  - `enable` reuses saved config

---

## 3. High-level architecture

`dot-ddns` has two roles:

1. **CLI**
   - initializes config
   - applies config one-shot
   - enables/disables runtime DoT management
   - shows status
   - detects backend

2. **Daemon**
   - runs continuously under systemd
   - resolves DoT provider hostname via bootstrap resolvers
   - tracks active links
   - reapplies per-link runtime DoT config whenever:
     - the resolved IP set changes
     - active links change
     - backend changes

### Core design choice

`dot-ddns` will not primarily modify persistent NetworkManager connection DNS settings.

Instead, it will apply **runtime per-link DNS configuration through `systemd-resolved`** using `resolvectl` or equivalent D-Bus APIs.

`NetworkManager` is used in v1 primarily for:

- backend detection
- active connection discovery
- active device/interface tracking

This avoids reconnecting interfaces and avoids overwriting user-managed persistent profile DNS settings.

---

## 4. Backend model

## 4.1 Supported backends

### Backend: `networkmanager`
Chosen when:

- `NetworkManager.service` is active, and
- `systemd-resolved.service` is active enough to accept per-link DNS configuration, and
- active managed connections/devices can be discovered

Responsibilities:

- enumerate all active managed connections
- determine the interface names and indices to manage
- monitor connection/device changes
- apply DoT to all active managed links through `systemd-resolved`

### Backend: `resolved`
Chosen when:

- `NetworkManager.service` is not active, and
- `systemd-resolved.service` is active

Responsibilities:

- enumerate relevant active links directly from the kernel/network stack
- monitor link changes
- apply DoT to those links through `systemd-resolved`

### Unsupported
If neither backend is usable, `dot-ddns` exits with a clear error.

---

## 5. DNS application model

## 5.1 Runtime application target

DoT is configured per link using `systemd-resolved` runtime configuration.

Runtime application uses the following `resolvectl` concepts:

- `resolvectl dns LINK SERVER...`
- `resolvectl dnsovertls LINK yes`
- `resolvectl domain LINK ~.`
- `resolvectl default-route LINK yes`
- `resolvectl revert LINK`

## 5.2 DoT endpoint syntax

Each DNS server entry must include the TLS server name.

Examples:

- `1.1.1.1#one.one.one.one`
- `9.9.9.9#dns.quad9.net`
- `[2606:4700:4700::1111]#one.one.one.one`

This ensures:

- upstream connection goes to the resolved IP
- TLS certificate validation is done against the provider hostname

## 5.3 Route-only domain

For each managed link, `dot-ddns` sets:

- `domain LINK ~.`
- `default-route LINK yes`

Intent:

- make the configured DoT servers eligible for global DNS traffic on the managed link
- prefer the managed resolver path for general lookups

## 5.4 Revert behavior

Disabling DoT or losing ownership of a link uses:

- `resolvectl revert LINK`

This removes all runtime per-link DNS settings applied via resolved runtime configuration.

---

## 6. Update model

## 6.1 Daemon approach

A long-running daemon is required to meet the “within a couple seconds” update target.

The daemon will:

- wake every `poll_interval`
- resolve the configured DoT hostname through bootstrap resolvers
- compare results to the last applied IP set
- reapply if changed
- separately monitor link/backend changes and reconcile immediately

## 6.2 Default timing

Default:

- `poll_interval = "2s"`
- per-bootstrap DNS timeout: `1s`
- small jitter optional in future, not required in v1

## 6.3 Behavior on resolution failure

If provider resolution fails:

- keep the last known good DoT configuration in place
- log the failure
- retry next cycle
- do not automatically clear DNS configuration

Rationale:

- failing closed by wiping DNS config can create an avoidable outage
- last known good state is preferable to self-induced loss of name resolution

---

## 7. Bootstrap resolution

## 7.1 Requirement

Provider hostname resolution must not depend on the current system resolver state.

Otherwise the system may be unable to discover the new DoT IP after the old DoT IP becomes invalid.

## 7.2 Rule

`dot-ddns` resolves the configured DoT hostname using explicitly configured bootstrap resolvers.

These bootstrap resolvers are plain DNS on port 53 in v1.

## 7.3 Bootstrap config

Bootstrap servers are configured in the config file.

Example:

```toml
bootstrap = [
  "9.9.9.9:53",
  "1.1.1.1:53",
  "[2620:fe::fe]:53",
  "[2606:4700:4700::1111]:53",
]
```

## 7.4 Resolution behavior

The resolver will:

- query A and AAAA
- use the bootstrap servers directly
- deduplicate answers
- sort into a stable canonical order
- treat the combined set as the desired upstream set

## 7.5 Canonical order

v1 canonical order:

1. all IPv4 addresses sorted ascending
2. all IPv6 addresses sorted ascending
3. final endpoint list = v4 endpoints followed by v6 endpoints

This ensures change detection is deterministic.

---

## 8. Link discovery

## 8.1 NetworkManager backend discovery

In `networkmanager` mode, managed links are the set of:

- active managed connections
- with an attached active device
- excluding loopback

Each managed link record includes:

- interface name
- interface index
- device type if available
- connection UUID
- connection ID

v1 manages **all active NetworkManager-managed links**.

## 8.2 Raw resolved backend discovery

In `resolved` mode, candidate links are discovered from kernel/network state.

Eligible links should be:

- not loopback
- operationally up or carrier-up
- preferably with at least one address, route, or DNS relevance

Minimum v1 heuristic:

- non-loopback
- interface state not `down`

Future refinement may incorporate default-route awareness.

## 8.3 Ownership model

The daemon maintains an internal owned link set.

When a link enters the owned set:

- apply DoT runtime config

When a link leaves the owned set:

- `revert LINK`

---

## 9. CLI specification

## 9.1 Commands

```text
dot-ddns init
dot-ddns enable
dot-ddns disable
dot-ddns apply
dot-ddns daemon
dot-ddns status
dot-ddns detect-backend
```

## 9.2 `init`

Purpose:

- create `/etc/dot-ddns/config.toml`
- validate user-provided domain and bootstrap resolvers
- optionally overwrite existing config with explicit flag

Suggested syntax:

```bash
dot-ddns init \
  --domain one.one.one.one \
  --bootstrap 9.9.9.9,1.1.1.1,[2620:fe::fe],[2606:4700:4700::1111]
```

Options:

- `--domain <FQDN>` required
- `--bootstrap <CSV>` required
- `--poll-interval <DURATION>` optional, default `2s`
- `--backend <auto|networkmanager|resolved>` optional, default `auto`
- `--force` optional
- `--config <PATH>` optional, default `/etc/dot-ddns/config.toml`

Effects:

- writes config file
- creates parent directory if needed
- does not require service start by default

## 9.3 `enable`

Purpose:

- validate config
- apply DoT immediately
- enable/start the systemd service unless runtime-only mode requested

Suggested syntax:

```bash
dot-ddns enable
```

Options:

- `--config <PATH>`
- `--runtime-only` do not invoke systemctl, only apply runtime state
- `--no-start` optional future flag, not required in v1

Effects:

- resolve provider domain through bootstrap
- discover backend
- discover managed links
- apply DoT runtime config to managed links
- save state
- if not runtime-only: `systemctl enable --now dot-ddns.service`

## 9.4 `disable`

Purpose:

- revert all managed runtime DoT configuration
- optionally stop/disable the service
- preserve config file

Suggested syntax:

```bash
dot-ddns disable
```

Options:

- `--config <PATH>`
- `--runtime-only` revert runtime config but do not touch systemd unit state

Effects:

- load state if available
- revert owned links
- clear owned runtime state in state file
- if not runtime-only: `systemctl disable --now dot-ddns.service`

## 9.5 `apply`

Purpose:

- perform a one-shot resolve + apply cycle without starting daemon mode

Suggested syntax:

```bash
dot-ddns apply
```

Options:

- `--config <PATH>`
- `--dry-run`

Effects:

- load config
- detect backend
- discover links
- resolve provider via bootstrap
- compare against current state
- apply if needed
- update state

## 9.6 `daemon`

Purpose:

- long-running process used by systemd service

Suggested syntax:

```bash
dot-ddns daemon --config /etc/dot-ddns/config.toml
```

Behavior:

- runs reconciliation loop forever
- handles signals gracefully
- reverts runtime state for owned links on shutdown where appropriate

## 9.7 `status`

Purpose:

- show current config, backend, service/runtime status, last resolved/applied IPs, and managed links

Suggested syntax:

```bash
dot-ddns status
```

Options:

- `--config <PATH>`
- `--json`

Output fields:

- config path
- domain
- bootstrap servers
- backend configured
- backend detected
- daemon poll interval
- last resolved IP set
- last applied endpoint set
- managed links
- whether runtime DoT appears active
- state file path

## 9.8 `detect-backend`

Purpose:

- print backend detection diagnostics

Suggested syntax:

```bash
dot-ddns detect-backend
```

Output fields:

- is NetworkManager active
- is systemd-resolved active
- chosen backend
- failure reason if none

---

## 10. Config file specification

Default path:

```text
/etc/dot-ddns/config.toml
```

## 10.1 Schema

```toml
domain = "one.one.one.one"
bootstrap = [
  "9.9.9.9:53",
  "1.1.1.1:53",
  "[2620:fe::fe]:53",
  "[2606:4700:4700::1111]:53",
]
poll_interval = "2s"
backend = "auto"
ip_family = "both"
log_level = "info"
```

## 10.2 Fields

### `domain`
- type: string
- required: yes
- must be a fully qualified domain name
- used both for address resolution and TLS server name

### `bootstrap`
- type: array of strings
- required: yes
- each item must be an IP[:port] or [IPv6]:port literal
- hostnames are not permitted in v1 for bootstrap entries
- default port is `53` if omitted by parser design; explicit ports recommended

### `poll_interval`
- type: duration string
- required: no
- default: `2s`
- minimum allowed in v1: `1s`

### `backend`
- type: enum
- values: `auto`, `networkmanager`, `resolved`
- default: `auto`

### `ip_family`
- type: enum
- values: `ipv4`, `ipv6`, `both`
- default: `both`

### `log_level`
- type: enum/string
- values: `error`, `warn`, `info`, `debug`, `trace`
- default: `info`

## 10.3 Validation rules

- `domain` must parse as a hostname, not IP literal
- `bootstrap` must not be empty
- `poll_interval` must be >= 1s
- if `ip_family = ipv4`, AAAA results are ignored
- if `ip_family = ipv6`, A results are ignored

---

## 11. State file specification

Default path:

```text
/var/lib/dot-ddns/state.json
```

## 11.1 Purpose

Stores tool runtime state for:

- last known good provider IPs
- last applied endpoint set
- last detected backend
- owned links
- timestamps

This is not intended to be a full user config backup.

## 11.2 Schema

```json
{
  "version": 1,
  "domain": "one.one.one.one",
  "backend": "networkmanager",
  "last_ips_v4": ["1.0.0.1", "1.1.1.1"],
  "last_ips_v6": ["2606:4700:4700::1111"],
  "last_endpoints": [
    "1.0.0.1#one.one.one.one",
    "1.1.1.1#one.one.one.one",
    "[2606:4700:4700::1111]#one.one.one.one"
  ],
  "managed_links": [
    {
      "ifindex": 2,
      "ifname": "enp5s0",
      "source": "networkmanager",
      "connection_id": "Wired connection 1",
      "connection_uuid": "00000000-0000-0000-0000-000000000000"
    }
  ],
  "enabled": true,
  "last_successful_resolve": "2026-04-21T16:00:00Z",
  "last_apply": "2026-04-21T16:00:00Z"
}
```

## 11.3 Behavior

- state file is written atomically
- if missing, tool recreates it
- if corrupt, tool logs warning and rebuilds from current run where possible

---

## 12. Daemon reconciliation algorithm

## 12.1 Main loop

Pseudo-flow:

1. Load config
2. Detect backend
3. Discover managed links
4. Resolve provider hostname via bootstrap resolvers
5. Build canonical endpoint list
6. Compare against stored endpoints
7. If endpoints changed, apply to all managed links
8. If managed link set changed, reconcile links:
   - apply to new links
   - revert removed links
9. Persist updated state
10. Sleep until next poll tick or react sooner to backend/link events

## 12.2 Reconciliation triggers

The daemon should reconcile on:

- periodic poll tick
- NetworkManager active connection/device signal
- netlink link change signal in resolved mode
- process startup
- receipt of SIGHUP or future manual reload trigger

## 12.3 Shutdown behavior

On normal shutdown:

- if stopping because service is being disabled, runtime config is reverted
- if the daemon simply restarts under systemd, service wrapper may use `ExecStopPost` to ensure cleanup as designed

v1 must avoid leaving stale ownership assumptions in state.

---

## 13. Backend detection specification

## 13.1 Detection order

1. Check `systemd-resolved.service`
2. Check `NetworkManager.service`
3. If NetworkManager is active, choose `networkmanager`
4. Else if resolved is active, choose `resolved`
5. Else fail

## 13.2 Forced backend

If config specifies:

- `backend = "networkmanager"`
  - fail if NetworkManager is unavailable
- `backend = "resolved"`
  - fail if resolved backend unavailable
- `backend = "auto"`
  - use detection order above

## 13.3 Failure messages

Must be explicit, e.g.

- `NetworkManager requested but service is not active`
- `systemd-resolved is not active; dot-ddns requires systemd-resolved for runtime DoT application`
- `no supported backend detected`

---

## 14. Resolved application semantics

## 14.1 Managed values per link

For each owned link, `dot-ddns` sets:

- DNS servers: full canonical endpoint set
- DNS-over-TLS: `yes`
- route-only domain: `~.`
- default-route: `yes`

## 14.2 Clearing values

Per-link runtime state is cleared by `revert LINK`.

v1 does not selectively merge runtime DNS with preexisting runtime link settings.

Ownership is exclusive at the runtime level for the links `dot-ddns` manages.

## 14.3 Idempotency

Apply operations must be idempotent:

- applying same endpoint set twice must not be treated as a change
- daemon should avoid unnecessary repeated command execution where feasible

---

## 15. Exit codes

Suggested exit codes:

- `0` success, no change
- `10` success, change applied
- `20` configuration error
- `21` backend unavailable
- `22` resolution failure
- `23` apply failure
- `24` permission error
- `25` state I/O error

Daemon mode may simply use non-zero exit on fatal errors and rely on systemd restart policy.

---

## 16. Logging

## 16.1 Output

- CLI commands log to stderr for human use
- `status` primary content goes to stdout
- daemon logs to journald through stdout/stderr

## 16.2 Log content examples

- backend selected
- bootstrap query success/failure
- new endpoint set detected
- link added/removed
- apply start/success/failure
- disable/revert operations

## 16.3 Sensitivity

- domain may be logged
- bootstrap IPs may be logged
- no secrets exist in v1, so no secret redaction issue beyond standard hygiene

---

## 17. Permissions

Commands requiring root:

- `enable`
- `disable`
- `apply` unless `--dry-run`
- `daemon`
- `init` when writing `/etc/dot-ddns/config.toml`

Commands allowed without root:

- `status` if only reading config/state/system status
- `detect-backend`
- `apply --dry-run`

If insufficient permissions are detected, return a clear permission error.

---

## 18. Systemd service specification

Unit path:

```text
/usr/lib/systemd/system/dot-ddns.service
```

## 18.1 Unit file

```ini
[Unit]
Description=Dynamic DNS-over-TLS updater for systemd-resolved
Documentation=man:dot-ddns(1)
After=network-online.target systemd-resolved.service
Wants=network-online.target
Requires=systemd-resolved.service

[Service]
Type=simple
ExecStart=/usr/bin/dot-ddns daemon --config /etc/dot-ddns/config.toml
ExecStopPost=/usr/bin/dot-ddns disable --runtime-only --config /etc/dot-ddns/config.toml
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
```

## 18.2 Notes

- no timer is shipped for v1
- service is disabled by default after package install
- user enables it explicitly

---

## 19. Filesystem layout

Installed paths:

```text
/usr/bin/dot-ddns
/usr/lib/systemd/system/dot-ddns.service
/usr/share/doc/dot-ddns/config.example.toml
/usr/share/licenses/dot-ddns/LICENSE
/etc/dot-ddns/config.toml            # user-created by init, not package-owned by default
/var/lib/dot-ddns/state.json         # runtime state
```

Optional tmpfiles entry:

```text
/usr/lib/tmpfiles.d/dot-ddns.conf
```

Content:

```text
d /var/lib/dot-ddns 0755 root root -
```

---

## 20. Repository layout

```text
dot-ddns/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── .gitignore
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── daemon.rs
│   ├── state.rs
│   ├── resolver.rs
│   ├── resolvedctl.rs
│   ├── links.rs
│   ├── error.rs
│   ├── systemd.rs
│   └── backend/
│       ├── mod.rs
│       ├── detect.rs
│       ├── networkmanager.rs
│       └── resolved.rs
├── docs/
│   └── implementation-spec.md
├── packaging/
│   ├── dot-ddns.service
│   ├── dot-ddns.install
│   ├── dot-ddns.tmpfiles
│   └── PKGBUILD
└── tests/
    └── integration/
```

---

## 21. Rust crate/dependency plan

Recommended dependencies:

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
hickory-resolver = "0.25"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env-filter"] }
zbus = "4"
rtnetlink = "0.14"
chrono = { version = "0.4", features = ["serde"] }
```

Notes:

- `hickory-resolver` for direct bootstrap DNS queries
- `zbus` for NetworkManager monitoring
- `rtnetlink` for raw link tracking
- shelling out to `resolvectl` is acceptable in v1 to simplify resolved application

---

## 22. Internal module responsibilities

## `main.rs`
- entrypoint
- top-level error formatting

## `cli.rs`
- clap command definitions
- CLI arg parsing

## `config.rs`
- config schema
- load/save/validate

## `daemon.rs`
- main reconciliation loop
- trigger wiring

## `state.rs`
- state schema
- atomic load/store

## `resolver.rs`
- bootstrap DNS client
- A/AAAA lookup
- canonical result generation

## `resolvedctl.rs`
- wrapper around `resolvectl`
- apply/revert/status helpers

## `links.rs`
- normalized link model
- owned set diffing

## `backend/detect.rs`
- backend detection logic

## `backend/networkmanager.rs`
- NM D-Bus discovery
- active managed link enumeration
- signal subscription

## `backend/resolved.rs`
- raw link enumeration
- netlink monitor support

## `systemd.rs`
- optional helpers for enabling/disabling service from CLI

## `error.rs`
- custom error types

---

## 23. Status command output specification

Human-readable output example:

```text
config: /etc/dot-ddns/config.toml
enabled: true
domain: one.one.one.one
backend configured: auto
backend detected: networkmanager
poll interval: 2s
bootstrap resolvers:
  - 9.9.9.9:53
  - 1.1.1.1:53
  - [2620:fe::fe]:53
  - [2606:4700:4700::1111]:53
current endpoints:
  - 1.0.0.1#one.one.one.one
  - 1.1.1.1#one.one.one.one
  - [2606:4700:4700::1111]#one.one.one.one
managed links:
  - enp5s0 (ifindex 2)
  - wlp2s0 (ifindex 3)
last successful resolve: 2026-04-21T16:00:00Z
last apply: 2026-04-21T16:00:00Z
```

JSON mode should expose equivalent structured fields.

---

## 24. Disable semantics

`disable` means:

1. stop managing runtime DoT configuration
2. revert all currently owned links
3. preserve config file
4. preserve enough state to support diagnostics, but set `enabled = false`

It does **not** mean deleting config unless a future `uninstall` command is added.

---

## 25. Failure handling

## 25.1 Bootstrap partial failure

If some bootstrap servers fail but at least one succeeds with usable records:

- continue
- log degraded condition

## 25.2 No records returned

If no usable A/AAAA records are returned for the selected family/families:

- do not clear current config
- treat as resolution failure

## 25.3 Link apply partial failure

If applying to one link fails but others succeed:

- continue attempting remaining links
- aggregate errors
- state should reflect only successfully owned links if partial success is represented
- daemon keeps retrying on subsequent cycles

## 25.4 Service restart

On daemon restart:

- recompute desired state from config and live system state
- do not trust stale state blindly
- converge to desired runtime configuration

---

## 26. Security and safety notes

- No secrets are stored in v1.
- Runtime DNS configuration is applied only to supported backends.
- Bootstrap resolvers are plain DNS; this is an accepted v1 tradeoff for bootstrapping reliability and simplicity.
- The daemon should avoid unbounded shell injection risk by invoking `resolvectl` with direct argument vectors, not shell strings.
- Input validation is required for all config fields.

---

## 27. Testing plan

## 27.1 Unit tests

- config parsing/validation
- bootstrap address parsing
- endpoint formatting
- canonical sort behavior
- state load/store
- diffing owned link sets

## 27.2 Integration tests

- one-shot apply with mocked `resolvectl`
- disable/revert behavior with mocked `resolvectl`
- daemon reconciliation with mocked resolver result changes
- NetworkManager backend discovery abstraction tests

## 27.3 Manual Arch tests

Target environments for v1:

1. Arch with `systemd-resolved` only
2. Arch with `NetworkManager` + `systemd-resolved`
3. IPv4-only network
4. IPv6-only or dual-stack network
5. Link flap while daemon running
6. Provider hostname IP change simulation

## 27.4 Acceptance criteria

- CLI can initialize config successfully
- `enable` applies DoT runtime config
- `status` reflects active runtime state
- daemon updates within a few seconds after provider IP change
- `disable` reverts runtime DoT config
- service survives restarts and converges back to desired state

---

## 28. AUR packaging specification

## 28.1 Package contents

- compiled binary
- service unit
- tmpfiles config
- documentation example config
- license

## 28.2 Packaging rules

- package name should match binary name: `dot-ddns`
- package should not auto-enable service on install
- package may print post-install guidance
- package should build using standard Rust packaging flow

## 28.3 PKGBUILD outline

Expected packaging flow:

- `cargo build --release --locked`
- install binary to `/usr/bin`
- install unit to `/usr/lib/systemd/system`
- install docs/example config
- install tmpfiles entry

## 28.4 Optional `.install` post-install messaging

Suggested messages:

- copy/create config with `dot-ddns init`
- inspect config
- enable service with `systemctl enable --now dot-ddns.service`

---

## 29. Documentation requirements

The repo should include:

- `README.md`
  - what the tool does
  - requirements
  - install from AUR/manual build
  - quick start
  - how enable/disable works
- `docs/implementation-spec.md`
  - this document
- example config
- troubleshooting section in README

---

## 30. Milestone plan

## Milestone 1: bootstrap project
- initialize git repo
- create cargo project
- add CLI skeleton
- add config + state schemas

## Milestone 2: one-shot core
- implement bootstrap resolver
- implement canonical endpoint generation
- implement resolved runtime apply/revert wrapper
- implement `apply`, `status`, `detect-backend`

## Milestone 3: backend discovery
- implement backend detection
- implement NetworkManager active link enumeration
- implement raw link enumeration

## Milestone 4: daemon
- implement reconciliation loop
- implement periodic polling
- implement state persistence and convergence logic

## Milestone 5: change monitoring
- implement NM D-Bus change monitoring
- implement raw link netlink monitoring
- ensure immediate reconciliation on link changes

## Milestone 6: packaging
- add systemd unit
- add example config
- add PKGBUILD and packaging files
- add README and usage docs

---

## 31. Non-goals for v1

The following are explicitly out of scope for v1 unless later required:

- DNS over HTTPS support
- Opportunistic DoT mode
- Persistent mutation of NetworkManager connection profile DNS settings
- Merging with arbitrary preexisting runtime DNS settings
- Support for non-systemd resolvers
- Non-Linux platform support
- Bootstrap over TLS/HTTPS

---

## 32. Implementation summary

v1 of `dot-ddns` is a Rust CLI + daemon that:

- resolves a configured DoT provider hostname via explicit bootstrap DNS resolvers
- configures all resolved IPs as DoT upstreams with the hostname as SNI/auth name
- detects `NetworkManager` vs raw `systemd-resolved`
- manages all active links
- applies DoT via `systemd-resolved` runtime per-link config
- continuously refreshes configuration every 2 seconds by default
- provides `enable`, `disable`, `apply`, `status`, and `detect-backend` commands
- is packaged as a normal git repo suitable for AUR publication
