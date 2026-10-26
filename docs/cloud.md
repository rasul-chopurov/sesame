# Sesame Cloud (self-hosted)

`sesame-cloud` is Sesame for a team: one binary that serves a web app (also
installable on phones as a PWA) and an API, on top of the same engine as the
desktop app. Plans, checks and the audit log are identical.

## Roles

| Role | Can |
|---|---|
| Owner | everything; adds admins |
| Admin | everything; adds developers, viewers and agents; approves requests; `run_command` |
| Developer | reads; runs changes; destructive changes wait for an admin |
| Viewer | reads only |
| Agent (token) | like its role, but **every** change waits for a person |

## Run it

```sh
cargo build --release -p sesame-cloud
(cd web && npm ci && npm run build)

SESAME_DATA_DIR=/var/lib/sesame \
SESAME_MASTER_PASSWORD='…' \
./target/release/sesame-cloud --listen 127.0.0.1:7880 --web web/dist
```

On first start the log prints a **setup code**; open the web app and create
the owner account with it. Servers are added with the `sesame` CLI on the
same machine (`SESAME_DATA_DIR=/var/lib/sesame sesame add prod root@1.2.3.4`)
and its SSH key must be in the server's `authorized_keys`.

Put it behind Caddy for HTTPS — it listens on 127.0.0.1 only by default:

```caddy
cloud.example.com {
	reverse_proxy 127.0.0.1:7880
}
```

systemd unit:

```ini
[Unit]
Description=Sesame Cloud
After=network-online.target

[Service]
User=sesame
Environment=SESAME_DATA_DIR=/var/lib/sesame
EnvironmentFile=/etc/sesame/cloud.env   # SESAME_MASTER_PASSWORD=…
ExecStart=/usr/local/bin/sesame-cloud --web /usr/local/share/sesame-cloud/web
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

## API

Everything the web app does is the API, with `Authorization: Bearer <token>`:

- `POST /api/login` → `{ token }`, `POST /api/logout`, `GET /api/me`
- `GET /api/tools`, `POST /api/tools/{name}` — the same 30 tools as the desktop MCP/REST
- `GET /api/approvals`, `POST /api/approvals/{id}/allow|deny`
- `GET /api/audit`, `GET|POST /api/users`, `PATCH|DELETE /api/users/{id}`, `POST /api/agents`

Agents get a token from **Team → Add an agent**.

## What is next

This is phase 1 of [the cloud plan](cloud-plan.md): one team per instance.
Multi-tenant hosting, billing, MCP over HTTP and native mobile apps follow.
