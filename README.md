<div align="center">

<img src="assets/logo.svg" width="112" alt="Sesame logo">

# Sesame

**Open your servers.**

A native, AI-friendly server manager for the Laravel community.<br>
Give it SSH — it finds every site, database, mail setup and process on the box, and lets you run them.

[![Release](https://img.shields.io/github/v/release/rasul-chopurov/sesame?include_prereleases&label=release&color=E07A2E)](https://github.com/rasul-chopurov/sesame/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-B8322A)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20·%20Windows%20(soon)-2A1B3D)](#install)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-F6B73C)](https://github.com/longbridge/gpui-kit)

**English** · [Русский](#русский)

</div>

<p align="center">
  <img src="docs/screenshots/overview-dark-en.png" width="49%" alt="Overview, dark glass theme">
  <img src="docs/screenshots/sites-light-en.png" width="49%" alt="Sites as searchable cards, light glass theme">
</p>

> **Alpha.** Sesame is at `0.1.0-alpha.1`. Discovery is solid and read-only; managing services, databases and sites is landing next — see the [roadmap](#roadmap).

## Why

Forge, Ploi and Coolify are web dashboards with an agent on your server. Sesame is the other way around: a **fast native app on your Mac** that talks to your servers over plain SSH. Nothing to install on the server, no Electron, no JVM — it starts instantly and stays light.

It is built for how Laravel actually runs in production today: FrankenPHP and Octane, Reverb, Horizon and queue workers under supervisor, Redis, MySQL or Postgres, Cloudflare in front.

## What it does today

| | |
|---|---|
| 🔑 **Connect** | ssh-agent, key file or password (kept in the macOS Keychain). Host keys verified against `~/.ssh/known_hosts`; a changed key refuses to connect. |
| 🔍 **Discover** | One read-only probe, two SSH round trips: OS and resources · systemd and supervisor (Octane, queue, Horizon, Reverb recognised) · listening ports · sites from **FrankenPHP/Caddy, Nginx, Apache** · Laravel and WordPress with non-secret `.env` details · MySQL, MariaDB, PostgreSQL, Redis, MongoDB · mail · PHP and FPM pools · cron · certificates and expiry · ufw and fail2ban · Docker. |
| 🚨 **Needs attention** | Failed processes, ports open to the world, inactive firewall, expiring certificates, PHP-FPM running for nobody, full disks. |
| 🤖 **AI-friendly** | Every action is a typed operation. `sesame discover root@host` prints JSON today; MCP and REST servers for agents are on the roadmap. |
| 🎨 **Glass UI, EN / RU / AZ** | Translucent light and dark themes over a blurred window, breadcrumbs in the titlebar, a sidebar tree with search, sites as searchable cards. English, Russian and Azerbaijani. |
| ⬆️ **Updates** | Checks GitHub Releases, shows the changelog of every version since yours, installs signed builds automatically if you want. |
| 🔒 **Safe by default** | Discovery never changes anything and never reads passwords or keys from `.env`. |

<p align="center">
  <img src="docs/screenshots/site-dark-ru.png" width="49%" alt="Site page in Russian">
  <img src="docs/screenshots/services-light-ru.png" width="49%" alt="Services catalog in Russian">
</p>

## Install

Prebuilt builds will appear on the [Releases](https://github.com/rasul-chopurov/sesame/releases) page. Until the app is notarized, clear the quarantine flag after copying it to Applications:

```sh
xattr -dr com.apple.quarantine /Applications/Sesame.app
```

### Build from source

Requires Rust (`rustup`) and Xcode on macOS (GPUI uses Metal).

```sh
git clone https://github.com/rasul-chopurov/sesame && cd sesame
cargo run -p sesame-app --release
```

## CLI

The same binary is a CLI — handy for scripts and AI agents:

```sh
sesame add prod root@203.0.113.10        # save a server (ssh-agent by default)
sesame add prod deploy@host:2222 --key ~/.ssh/id_ed25519
sesame servers                           # list saved servers
sesame discover prod                     # discover, save, print JSON
sesame discover root@203.0.113.10        # ad-hoc, nothing saved
sesame ops                               # operations exposed to agents
```

State lives in `~/Library/Application Support/dev.sesame.Sesame/state.db` (override with `SESAME_DATA_DIR`). Secrets never touch it.

## Roadmap

- [x] SSH, read-only discovery, insights, light and dark themes, CLI
- [x] Glass design, Russian translation, sites as searchable cards, in-app updates with changelog
- [ ] Command palette ⌘K
- [ ] Maintenance mode · disable/enable · per-site isolation (own Linux user, PHP-FPM pool or Octane, DB user, Redis ACL)
- [x] Start/stop/restart services, clean up unused ones — previewed, confirmed, audited
- [x] Databases: create/drop databases and users for MySQL, MariaDB, PostgreSQL, MongoDB (backup before drop)
- [x] One-click services: Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3 (RustFS), Reverb
- [ ] Cloudflare: DNS, cache purge, Origin certificates
- [ ] Deploys and a "new Laravel site" wizard
- [ ] MCP and REST API for AI agents
- [ ] Provision a fresh server from zero
- [ ] Windows · mobile · Sesame Cloud

## Architecture

```text
crates/
  sesame-update      GitHub Releases updates, ed25519 signatures, install
  sesame-core        data model, SQLite store, keychain secrets, operation registry
  sesame-ssh         SSH transport (russh): auth, known_hosts, exec
  sesame-discovery   read-only probe + parsers + insights
  sesame-ops         plans, runner, audit: service control, databases, maintenance
  sesame-provision   service recipes (install / uninstall)
  sesame-app         GPUI window and the `sesame` CLI
```

Only `sesame-app` depends on GPUI, so the mobile app, the agent APIs and the cloud reuse everything else.

## Contributing

Issues and pull requests are welcome — read [CONTRIBUTING.md](CONTRIBUTING.md). Security reports: [SECURITY.md](SECURITY.md).

## License

[Apache-2.0](LICENSE). Self-hosted and open source; a hosted Sesame Cloud will come later.

---

## Русский

**Sesame — «Сим-сим, откройся» для ваших серверов.** Нативное приложение на Rust + GPUI для Laravel-сообщества: даёте SSH-доступ, Sesame сам находит сайты, базы, почту, процессы и показывает, что требует внимания. Без агента на сервере, без Electron.

- **Сейчас:** подключение по SSH, обнаружение только на чтение, наблюдения, стеклянные светлая и тёмная темы, русский интерфейс, сайты-карточки с поиском, автообновление с changelog, CLI.
- **Дальше:** изоляция сайтов, установка сервисов (Redis, PostgreSQL, MariaDB, MongoDB, Elasticsearch, S3, Reverb), базы данных, Cloudflare, MCP для ИИ-агентов, автообновление.

Сборка: `cargo run -p sesame-app --release`. Лицензия Apache-2.0.
