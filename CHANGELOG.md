# Changelog

All notable changes to Sesame are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and Sesame uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed (found on a real Ubuntu 24.04 arm64 server)
- Elasticsearch 9 listened on all interfaces because its security auto-config writes `http.host: 0.0.0.0`; the recipe now pins network, http and transport hosts to 127.0.0.1.
- MongoDB scripts reported success on errors (`mongosh` with a heredoc exits 0); scripts now run through `mongosh --file` and authenticate as the Sesame admin.
- Reinstalling MongoDB over kept data no longer tries to create a second admin or reveals a password that was never set.
- Every plan step now sees `/etc/os-release` (each step runs in a fresh shell).
- Backups are written with `umask 077`.
- PostgreSQL version shown as `18.6` instead of the package string.

### Changed
- Secrets moved from the macOS Keychain to an encrypted vault in Sesame's own database: the key is derived from a master password with Argon2id (64 MiB, 3 passes) and every secret is sealed with XChaCha20-Poly1305 bound to its slot. Optional key-file mode without a password. Works the same on macOS and Windows and survives app updates.

### Added
- Sesame Glass design: translucent light and dark themes over a blurred window, breadcrumbs in the titlebar, sidebar tree with server/site search, status bar, brand icons for detected software.
- Russian and Azerbaijani translations; language and theme follow the system or are chosen in Settings.
- Changes on servers, always previewed and confirmed (destructive ones by typing the name), run with a live log and recorded in an audit log:
  - start / stop / restart / disable systemd services and supervisor programs; one-click "stop unused PHP-FPM";
  - create databases with a dedicated user and generated password (MySQL, MariaDB, PostgreSQL, MongoDB); drop databases after an automatic backup to `/var/backups/sesame`;
  - install and uninstall Redis, PostgreSQL (PGDG), MariaDB, MongoDB 8.0, Elasticsearch 9 and RustFS (S3) with localhost-only defaults; uninstall keeps data unless asked;
  - Laravel maintenance mode on/off and Reverb under supervisor per site.
- `sesame do <server> …` runs the same plans from the terminal (install, uninstall, db-create, db-drop, service, maintenance, reverb); `sesame audit` shows the log; `sesame secret-set` stores a known credential.
- Generated credentials are shown once, copyable as `.env`, and stored in the encrypted vault.
- Sites as searchable, filterable cards with a detail page (framework, database, drivers, processes, run-as users).
- Services page showing which of Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 (RustFS/MinIO) and Reverb are installed, with conflicts.
- Settings: theme, language, update channel, automatic updates; "What's new" page with release notes.
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
