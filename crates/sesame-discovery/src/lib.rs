//! Discovery: connect once, run a read-only probe, turn the output into a
//! [`ServerSnapshot`]. Nothing on the server is changed.

pub mod insights;
pub mod parse;
pub mod script;

use anyhow::Result;
use sesame_core::snapshot::*;
use sesame_ssh::Exec;

use parse::{app, system, web};

/// Runs both probe phases over `exec` and builds the snapshot.
pub async fn discover<E: Exec>(exec: &E) -> Result<ServerSnapshot> {
    let out = exec.exec(script::PROBE).await?;
    let mut snap = from_probe(&out.stdout);

    let (apps, certs) = detail_targets(&snap);
    if !apps.is_empty() || !certs.is_empty() {
        let details = exec.exec(&script::details(&apps, &certs)).await?;
        apply_details(&mut snap, &details.stdout);
    }
    link(&mut snap);
    Ok(snap)
}

/// Phase 1 output → snapshot (sites without app details yet).
pub fn from_probe(output: &str) -> ServerSnapshot {
    let sections = script::sections(output);
    let get = |name: &str| -> &str {
        sections.iter().find(|(n, _)| *n == name).map(|(_, b)| *b).unwrap_or("")
    };

    let (mem_total, mem_avail) = system::meminfo(get("meminfo"));
    let (disk_total, disk_used) = system::df(get("df"));
    let mut uname = get("uname").lines();
    let privileged = get("privileged").trim() == "yes";
    let services = system::systemd(get("systemd"));
    let ports = system::ports(get("ports"));

    let mut snap = ServerSnapshot {
        taken_at: app::now_rfc3339(),
        os: OsInfo {
            hostname: get("hostname").trim().to_string(),
            distro: system::os_release(get("os_release")),
            kernel: uname.next().unwrap_or("").trim().to_string(),
            arch: uname.next().unwrap_or("").trim().to_string(),
            cpus: get("nproc").trim().parse().unwrap_or(0),
            mem_total_bytes: mem_total,
            mem_available_bytes: mem_avail,
            disk_total_bytes: disk_total,
            disk_used_bytes: disk_used,
            uptime_seconds: system::uptime(get("uptime")),
            load: system::loadavg(get("loadavg")),
            privileged,
        },
        supervisor: system::supervisor(get("supervisor_status"), get("supervisor_conf")),
        php: system::php(get("php"), get("php_fpm"), &services),
        databases: system::databases(&get, &ports),
        mail: system::mail(get("mail"), get("postfix")),
        cron: system::cron(get("cron_users"), get("cron_d")),
        firewall: system::firewall(get("ufw"), get("fail2ban")),
        containers: system::containers(get("docker")),
        tools: get("sesame_tools").lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
        services,
        ports,
        ..Default::default()
    };
    if !privileged {
        snap.warnings.push(
            "Not root and no passwordless sudo: supervisor, port owners, nginx -T and other users' crontabs may be missing."
                .into(),
        );
    }

    // Web servers and their sites.
    let bins: Vec<&str> = get("web_bins").lines().map(str::trim).collect();
    let caddy_kind = if bins.contains(&"frankenphp") { WebServerKind::FrankenPhp } else { WebServerKind::Caddy };
    if bins.contains(&"frankenphp") || bins.contains(&"caddy") {
        let version = if caddy_kind == WebServerKind::FrankenPhp { get("frankenphp_version") } else { get("caddy_version") };
        snap.web_servers.push(WebServer {
            kind: caddy_kind,
            version: first_line(version),
            config_path: first_line(get("caddy_config_path")),
        });
        snap.sites.extend(web::caddy(get("caddy_config"), caddy_kind));
        if get("caddy_config").trim().is_empty() {
            snap.warnings.push("Caddy config not readable (admin API off and adapt failed).".into());
        }
    }
    if bins.contains(&"nginx") {
        snap.web_servers.push(WebServer {
            kind: WebServerKind::Nginx,
            version: get("nginx_version").split('/').nth(1).map(|v| v.trim().to_string()),
            config_path: Some("/etc/nginx/nginx.conf".into()),
        });
        snap.sites.extend(web::nginx(get("nginx_dump")));
    }
    if bins.contains(&"apache2") || bins.contains(&"httpd") {
        snap.web_servers.push(WebServer {
            kind: WebServerKind::Apache,
            version: get("apache_version").split('/').nth(1).and_then(|v| v.split_whitespace().next()).map(String::from),
            config_path: None,
        });
        snap.sites.extend(web::apache(get("apache_conf")));
    }
    // A turned-off site shows up twice: its 503 placeholder block and the
    // set-aside file. Keep one entry, marked disabled, with the real root.
    for off in web::disabled(get("sesame_disabled")) {
        match snap.sites.iter_mut().find(|s| s.domains.first() == off.domains.first()) {
            Some(site) => {
                site.disabled = true;
                site.root = off.root.clone();
            }
            None => snap.sites.push(off),
        }
    }
    for site in &mut snap.sites {
        site.app_path = site.root.as_deref().map(app_path);
    }

    // Certificates referenced by sites, plus Let's Encrypt lineages.
    for name in get("letsencrypt").lines().map(str::trim).filter(|n| !n.is_empty() && *n != "README") {
        snap.certificates.push(Certificate {
            path: format!("/etc/letsencrypt/live/{name}/fullchain.pem"),
            subject: None,
            issuer: None,
            names: vec![name.to_string()],
            not_after: None,
        });
    }
    snap
}

/// First non-empty line, without build hashes (`h1:…`) Caddy appends.
fn first_line(s: &str) -> Option<String> {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(line.split_whitespace().filter(|w| !w.starts_with("h1:")).collect::<Vec<_>>().join(" "))
}

/// `/var/www/app/public/` → `/var/www/app`.
fn app_path(root: &str) -> String {
    let r = root.trim_end_matches('/');
    r.strip_suffix("/public").or_else(|| r.strip_suffix("/web")).unwrap_or(r).to_string()
}

fn detail_targets(snap: &ServerSnapshot) -> (Vec<String>, Vec<String>) {
    let mut apps: Vec<String> = Vec::new();
    for p in snap.sites.iter().filter_map(|s| s.app_path.clone()) {
        if !p.starts_with("/usr/share/") && !apps.contains(&p) {
            apps.push(p);
        }
    }
    let mut certs: Vec<String> = snap.certificates.iter().map(|c| c.path.clone()).collect();
    for t in snap.sites.iter().filter_map(|s| s.tls.clone()) {
        if t.starts_with('/') && !certs.contains(&t) {
            certs.push(t);
        }
    }
    (apps, certs)
}

/// Phase 2 output → app details and certificate data.
pub fn apply_details(snap: &mut ServerSnapshot, output: &str) {
    for (name, body) in script::sections(output) {
        if let Some(path) = name.strip_prefix("app:") {
            let d = app::app(body);
            for site in snap.sites.iter_mut().filter(|s| s.app_path.as_deref() == Some(path)) {
                site.framework = d.framework;
                site.framework_version = d.version.clone();
                site.env = d.env.clone();
                site.git = d.git.clone();
                site.owner = d.owner.clone();
                site.access = d.access.clone();
            }
        } else if let Some(path) = name.strip_prefix("cert:") {
            let Some(cert) = app::certificate(path, body) else { continue };
            match snap.certificates.iter_mut().find(|c| c.path == path) {
                Some(existing) => *existing = cert,
                None => snap.certificates.push(cert),
            }
        }
    }
}

/// Connects the pieces: processes to sites, external mailers.
pub fn link(snap: &mut ServerSnapshot) {
    for site in &mut snap.sites {
        let Some(path) = site.app_path.clone() else { continue };
        site.processes = snap
            .supervisor
            .iter()
            .filter(|p| {
                p.directory.as_deref() == Some(path.as_str())
                    || p.command.as_deref().is_some_and(|c| c.contains(&format!("{path}/")))
            })
            .map(|p| p.name.clone())
            .collect();
    }
    if snap.mail.servers.is_empty() {
        for env in snap.sites.iter().filter_map(|s| s.env.as_ref()) {
            let Some(mailer) = env.mail_mailer.as_deref() else { continue };
            let desc = match env.mail_host.as_deref() {
                Some(h) if mailer == "smtp" => format!("smtp via {h}"),
                _ => mailer.to_string(),
            };
            if !snap.mail.external.contains(&desc) {
                snap.mail.external.push(desc);
            }
        }
    }
}

#[cfg(test)]
mod tests;

/// Connects to `server`, runs discovery and disconnects.
pub async fn discover_server(
    server: &sesame_core::ServerConfig,
) -> Result<(ServerSnapshot, sesame_ssh::HostKeyStatus)> {
    let session = sesame_ssh::Session::connect(server).await?;
    let snap = discover(&session).await;
    let host_key = session.host_key.clone();
    let _ = session.close().await;
    Ok((snap?, host_key))
}
