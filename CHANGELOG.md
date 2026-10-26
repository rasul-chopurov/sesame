# Changelog

All notable changes to Sesame are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Sesame uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- In-app updates from GitHub Releases: SemVer channels (stable / beta), changelog of every version since yours, ed25519-signed archives, atomic `.app` replacement with rollback.
- `sesame update [--install] [--channel stable|beta]`.
- Release pipeline: signed macOS (arm64, x64) `.dmg`/`.zip` and experimental Windows builds on every `v*` tag; app icon.

## [0.1.0-alpha.1] - 2026-10-26

First public alpha.

### Added
- Native macOS app (Rust + GPUI): servers sidebar, Overview / Sites / Processes / Databases / Mail & Cron / Security views, light and dark themes.
- SSH connection with ssh-agent, key file or password (stored in the macOS Keychain); host keys checked against `~/.ssh/known_hosts`.
- Read-only discovery in two SSH round trips: OS and resources, systemd and supervisor (Octane, queue, Horizon, Reverb roles), listening ports, sites from FrankenPHP/Caddy, Nginx and Apache, Laravel/WordPress detection with non-secret `.env` details, MySQL/MariaDB/PostgreSQL/Redis/MongoDB, mail, PHP and FPM pools, cron, certificates with expiry, ufw/fail2ban, Docker.
- "Needs attention" insights: failed processes, open ports, inactive firewall, expiring certificates, unused PHP-FPM, full disk.
- `sesame` CLI: `add`, `servers`, `remove`, `discover` (JSON), `import-snapshot`, `ops`.

[Unreleased]: https://github.com/rasul-chopurov/sesame/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/rasul-chopurov/sesame/releases/tag/v0.1.0-alpha.1
