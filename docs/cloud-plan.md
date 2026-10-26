# Sesame Cloud — plan

Status: October 2026. Phase 1 exists as self-hosted `sesame-cloud` + `web/` (see [cloud.md](cloud.md)); hosting for many teams is not built yet.

Sesame Cloud is the hosted, team version of Sesame: a Vue web app and a public
API in front of a central service that manages customers' servers over SSH.
The desktop app stays free and open source; Cloud charges for hosting,
teams and always-on features.

## 1. Principles

1. **Same engine everywhere.** The desktop app, the CLI, MCP, the REST API and
   Cloud all run the same `sesame-engine` tools and `sesame-ops` plans. Cloud is
   not a rewrite; it is the engine behind a multi-tenant API.
2. **Still nothing on the server.** Cloud connects over SSH from fixed egress
   IPs with a per-team key. An optional outbound "connector" comes later for
   servers behind NAT.
3. **Every change is a plan.** Preview, approve, run, audit — for people and for
   agents, now with team approvals.
4. **Self-hostable.** The same Cloud ships as a Docker Compose bundle (like
   Coolify) for companies that want it inside their own network.

## 2. Architecture

```text
 Browser (Vue 3 SPA)      Desktop app        Agents (MCP / REST)      Mobile
        │                      │                     │                   │
        └──────────── HTTPS ───┴──── api.sesame.cloud ┴───────────────────┘
                                      │
                    ┌─────────────────┴──────────────────┐
                    │  sesame-cloud (Rust, axum)          │
                    │  auth · teams · RBAC · billing      │
                    │  /v1 REST + OpenAPI · MCP over HTTP │
                    │  WebSocket: job logs, terminal, approvals
                    └───────┬───────────────┬─────────────┘
                            │               │
                     PostgreSQL         Redis (queues, pub/sub, rate limits)
                            │
                    ┌───────┴───────────────────────────┐
                    │  runners (Rust, horizontally scaled)│
                    │  SSH sessions · discovery · plans   │
                    │  fixed egress IPs per region        │
                    └───────┬───────────────────────────┘
                            │ SSH (per-team key)
                     customer servers
```

- **API (`sesame-cloud`)**: a new binary crate on axum. It owns accounts,
  teams, permissions and billing, and turns requests into jobs. It reuses
  `sesame-engine::TOOLS` for the tool list and JSON schemas, so the REST
  surface, the OpenAPI document and the MCP tools are generated from one place.
- **Runners**: workers that take jobs from Redis, hold SSH sessions, run
  `sesame_ops::execute` and stream output back over Redis pub/sub. One job per
  server at a time (a per-server lock), many servers in parallel.
- **Scheduler**: periodic discovery (every 15 min by default), certificate and
  disk checks, alerts, scheduled backups.
- **Storage**: PostgreSQL for everything durable (snapshots as JSONB), Redis for
  queues and live streams, S3-compatible storage for backups and logs.

## 3. Secrets

- Envelope encryption: each team has a data key; data keys are wrapped by a
  master key in a KMS (or an HSM-backed key for self-hosted: age/Tink with a
  sealed key file). Secrets are XChaCha20-Poly1305 as in the desktop vault.
- **SSH keys**: Cloud generates an ed25519 key per team. Users add the public
  key to `authorized_keys` (one copy-paste command, or the desktop app does it
  for them). Private keys never leave the runners' memory unencrypted.
- Generated passwords (databases, Redis ACL users) are shown once, then
  readable only by roles with "reveal secrets".
- Customer-side option: **bring your own key**, where Cloud only ever gets a
  short-lived SSH certificate signed by the customer's CA.

## 4. Users, teams and access

Learned from FASTPANEL: the Linux user is the isolation unit, the owner is a
first-class field of a site, and delegated access is a separate concept from
ownership. Sesame keeps all three — and fixes FASTPANEL's gap: changing a
site's owner is supported (chown files, move the FPM pool, crontab and DB
grants in one plan).

**Cloud roles (per team)**

| Role | Can |
|---|---|
| Owner | everything, billing, delete team |
| Admin | all servers, invite people, approve destructive plans |
| Developer | read all, run Change plans on assigned servers/sites, deploy |
| Viewer | read only, no secrets |
| Site delegate | only the sites they are given: files, database, cron, deploys, logs |

**Server-side accounts** (what a role can create on a server)

- Site owner = Linux user `s_<slug>` (exists today). Several sites may belong to
  one owner when the user asks for it ("client" grouping), default is one per site.
- SFTP access for a person = their SSH public key added to the owner's
  `authorized_keys` with `restrict` and an OpenSSH `Match User` chroot, so no
  extra FTP daemon is needed.
- Database users per site (exists today), with a "read-only user" option.
- Mailboxes per domain (see the mail plan below).

**Approvals**: plans of risk Destructive need an Admin's approval, optionally
two-person. Agents' plans always need a human. Approvals arrive in the web app,
on mobile (push) and in the desktop app.

## 5. API

- `POST /v1/auth/*` — email + password with passkeys, GitHub/Google OAuth, TOTP.
- `GET /v1/teams/{team}/servers`, `POST …/servers` (adds a server, returns the
  command to install the team key), `POST …/servers/{id}/discover`.
- `GET /v1/tools`, `POST /v1/teams/{team}/tools/{name}` — the same tools as
  desktop MCP/REST (`list_sites`, `create_site`, `install_service`, …).
- `GET /v1/jobs/{id}` and `WS /v1/jobs/{id}/stream` — status and live log.
- `GET /v1/approvals`, `POST /v1/approvals/{id}/{allow|deny}`.
- `WS /v1/servers/{id}/terminal` — PTY over WebSocket (role-gated, recorded).
- Tokens: personal tokens and team tokens with scopes (`read`, `change`,
  `destructive`, per server), expiry and an IP allow-list.
- **MCP over streamable HTTP** at `https://api.sesame.cloud/mcp` with OAuth, so
  Claude and others connect to a team without running anything locally.
- OpenAPI 3.1 generated from the tool registry; an official TypeScript client
  generated from it is what the Vue app uses.

## 6. Web app (Vue)

- **Stack**: Vue 3 + TypeScript + Vite, Vue Router, Pinia, TanStack Query for
  server state, a typed client generated from OpenAPI, xterm.js for the
  terminal, Monaco (or CodeMirror 6) for the file editor, Tailwind with the
  Sesame Glass tokens so web and desktop look alike. Locales en/ru/az from the
  same `app.yml` (converted at build time).
- **Pages** mirror the desktop app: Servers → Overview · Sites (cards, owner,
  PHP, status) · Site (repo, deploys, env, files, terminal, DNS, mail) ·
  Services · Databases · Processes · Security · Cloudflare · Mail · Team ·
  Approvals · Audit · Billing.
- Live updates over one WebSocket per tab (jobs, approvals, server health).
- Desktop app gets "Sign in to Sesame Cloud": servers sync both ways, local-only
  servers stay local.

## 7. Billing

- Merchant of record (Paddle or Lemon Squeezy) — Stripe does not support
  sellers in Azerbaijan; a MoR also handles VAT.
- Draft plans: **Free** (1 server, 1 seat), **Pro** (per server/month,
  unlimited sites, scheduled backups, alerts), **Team** (seats, roles,
  approvals, SSO, audit export). Self-hosted Cloud: free for personal use,
  a yearly licence for companies (open-core, Apache-2.0 engine).

## 8. Operations

- Regions: start with one EU region (Hetzner, Falkenstein/Helsinki), fixed
  egress IPs published for firewalls.
- Observability: OpenTelemetry traces per job, structured logs without secrets,
  per-team rate limits.
- Abuse: SSH connections only to servers the team proved control of (the key
  install command, or a TXT record for the IP's domain).
- Backups of Cloud itself: PostgreSQL PITR, encrypted, restore drills monthly.

## 9. Phases

| Phase | Scope | Rough size |
|---|---|---|
| 0 | Extract `sesame-engine` (done), make plans and discovery runnable without the desktop store (trait for storage) | 1–2 weeks |
| 1 | `sesame-cloud` API: auth, teams, servers, runners, jobs, tools, audit; Vue app with Servers/Overview/Sites/Services/Databases | 6–8 weeks |
| 2 | Roles, delegates, approvals, terminal and files over WebSocket, Cloudflare/GitHub per team, MCP over HTTP | 4–6 weeks |
| 3 | Billing, scheduled discovery and alerts, backups to S3, desktop sync | 4 weeks |
| 4 | Mobile app (approvals, alerts, overview), self-hosted Cloud bundle, connector for NAT servers | 6+ weeks |

## 10. Risks

- **Holding SSH keys for customers** is the core risk: keep runners isolated,
  keys encrypted in memory where possible, short-lived certificates as the
  recommended mode, and a clear incident runbook.
- **Long-running plans** (installs, deploys) must survive a runner restart:
  every step is idempotent or records progress in the job.
- **Concurrent edits** between desktop and Cloud: the server is the source of
  truth; rediscover before every plan and refuse when the snapshot is stale.
