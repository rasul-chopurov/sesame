# Where we stopped — 26 October 2026

Release: **v0.1.0-alpha.1** (first public alpha), tagged from `main`.

## Done

- Desktop app (macOS, Rust + GPUI): discovery, insights, sites with isolation,
  new-site wizard, subdomains, services catalog (Redis, PostgreSQL, MariaDB,
  MongoDB, Elasticsearch, RustFS, Stalwart, Reverb, phpMyAdmin, pgAdmin),
  databases, processes, mail & cron, security, terminal, site files, GitHub
  deploys, Cloudflare DNS, Resend mail, per-person SSH/SFTP access, turning
  sites off/on, "prepare a server for Laravel", ⌘K, EN/RU/AZ, light/dark,
  encrypted vault, signed in-app updates.
- Agents: one tool registry (`sesame-engine`) behind the CLI, MCP
  (`sesame mcp`) and REST (`sesame serve`); every change waits for approval.
- Sesame Cloud, self-hosted (`sesame-cloud` + `web/`): the web app mirrors the
  desktop, with roles (owner/admin/developer/viewer), agent tokens, approvals,
  audit, terminal over WebSocket. See [cloud.md](cloud.md).
- Landing page at https://sesame.response.az (EN/AZ/RU).
- Windows build passes in CI.

## Open / next

1. **Signing key**: add the GitHub secret `SESAME_SIGNING_KEY` (value in
   `~/.sesame/release-signing.key`). Without it releases are unsigned and the
   in-app updater refuses them. After adding it, re-run the release workflow.
2. **Web**: phpMyAdmin/pgAdmin through Sesame Cloud (proxy the SSH tunnel);
   binary file upload in the web file manager; deploy Cloud to a server
   (e.g. `cloud.response.az`) behind Caddy so it works from phones.
3. **Live checks not done yet**: Stalwart install on the test VPS (needs the
   user's OK), Resend with a real token, web-initiated installs/deploys.
4. Hosted multi-tenant Cloud, native mobile apps — [cloud-plan.md](cloud-plan.md).
5. macOS notarization (needs an Apple Developer ID).

## Notes

- The project folder was renamed in Finder from `~/Desktop/Desktoptools` to
  `~/Desktop/untitled folder 2` — rename it back if that was an accident.
- Test server: 167.233.224.19 (Hetzner CAX11, Ubuntu 24.04). Sites there:
  sesame.response.az (landing, deployed with `git pull` as
  `s_sesame_response_az`), lara.response.az (Laravel test). ufw is on
  (22, 80, 443).
- Local Sesame Cloud runs at http://127.0.0.1:7880 on the desktop's data
  (`pkill sesame-cloud` to stop; start with
  `./target/release/sesame-cloud --web web/dist`).
