//! Things worth the user's attention, derived from a snapshot. Shown on the
//! overview and returned to agents, so both see the same list.

use serde::Serialize;
use sesame_core::snapshot::*;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Insight {
    pub level: Level,
    /// Stable identifier, also the translation key (`insight.<code>`).
    pub code: &'static str,
    /// Values for `%{name}` placeholders in the translated text.
    pub params: Vec<(&'static str, String)>,
    /// English text, for agents and logs.
    pub message: String,
}

pub fn insights(snap: &ServerSnapshot, now: OffsetDateTime) -> Vec<Insight> {
    let mut out = Vec::new();
    let mut push = |level, code: &'static str, params: Vec<(&'static str, String)>, message: String| {
        out.push(Insight { level, code, params, message })
    };

    for p in snap.supervisor.iter().filter(|p| p.state != "RUNNING") {
        push(Level::Critical, "process_down", vec![("name", p.name.clone()), ("state", p.state.clone())], format!("Process {} is {}", p.name, p.state));
    }
    for s in snap.services.iter().filter(|s| s.active == "failed") {
        push(Level::Critical, "service_failed", vec![("name", s.unit.clone())], format!("Service {} failed", s.unit));
    }

    let disk = percent(snap.os.disk_used_bytes, snap.os.disk_total_bytes);
    if disk >= 90 {
        push(Level::Critical, "disk_full", vec![("percent", disk.to_string())], format!("Disk is {disk}% full"));
    } else if disk >= 80 {
        push(Level::Warning, "disk_full", vec![("percent", disk.to_string())], format!("Disk is {disk}% full"));
    }
    let mem_used = snap.os.mem_total_bytes.saturating_sub(snap.os.mem_available_bytes);
    let mem = percent(mem_used, snap.os.mem_total_bytes);
    if mem >= 90 {
        push(Level::Warning, "memory_high", vec![("percent", mem.to_string())], format!("Memory is {mem}% used"));
    }

    if snap.firewall.ufw.as_deref() == Some("inactive") {
        push(Level::Warning, "ufw_inactive", vec![], "Firewall (ufw) is inactive — only the provider's firewall, if any, protects open ports".into());
    }
    for p in snap.ports.iter().filter(|p| p.public && ![22, 80, 443].contains(&p.port)) {
        push(
            Level::Warning,
            "port_public",
            vec![("port", p.port.to_string()), ("process", p.process.clone())],
            format!("Port {} ({}) listens on all interfaces", p.port, p.process),
        );
    }
    if snap.firewall.fail2ban_jails.is_empty() && snap.ports.iter().any(|p| p.port == 22 && p.public) {
        push(Level::Info, "ssh_no_fail2ban", vec![], "SSH is public and fail2ban has no jails".into());
    }

    for c in &snap.certificates {
        let Some(exp) = c.not_after.as_deref().and_then(|d| OffsetDateTime::parse(d, &Rfc3339).ok()) else {
            continue;
        };
        let days = (exp - now).whole_days();
        if days < 0 {
            push(Level::Critical, "cert_expired", vec![("path", c.path.clone())], format!("Certificate {} expired", c.path));
        } else if days < 21 {
            push(Level::Warning, "cert_expiring", vec![("path", c.path.clone()), ("days", days.to_string())], format!("Certificate {} expires in {days} days", c.path));
        }
    }

    let fpm_used = snap.sites.iter().any(|s| s.php && s.served_by != Some(WebServerKind::FrankenPhp));
    for php in snap.php.iter().filter(|p| p.fpm_running) {
        if !fpm_used {
            push(
                Level::Info,
                "fpm_unused",
                vec![("version", php.version.clone())],
                format!("PHP {} FPM is running but no site uses it — it can be stopped to free memory", php.version),
            );
        }
    }

    for site in &snap.sites {
        let Some(env) = &site.env else { continue };
        if env.app_env.as_deref() == Some("local") || env.app_env.as_deref() == Some("development") {
            push(
                Level::Warning,
                "app_env_dev",
                vec![("site", site.domains.join(", ")), ("env", env.app_env.clone().unwrap_or_default())],
                format!("{} runs with APP_ENV={}", site.domains.join(", "), env.app_env.as_deref().unwrap_or("")),
            );
        }
    }
    out.sort_by(|a, b| b.level.cmp(&a.level));
    out
}

fn percent(part: u64, whole: u64) -> u64 {
    if whole == 0 { 0 } else { part * 100 / whole }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn flags_the_usual_suspects() {
        let mut snap = ServerSnapshot::default();
        snap.firewall.ufw = Some("inactive".into());
        snap.ports.push(ListeningPort { address: "*".into(), port: 8000, process: "frankenphp".into(), public: true });
        snap.ports.push(ListeningPort { address: "*".into(), port: 443, process: "frankenphp".into(), public: true });
        snap.php.push(PhpInstall { version: "8.5.11".into(), cli: true, fpm_running: true, fpm_pools: vec![] });
        snap.sites.push(Site { php: true, served_by: Some(WebServerKind::FrankenPhp), ..Default::default() });
        snap.supervisor.push(SupervisorProgram {
            name: "worker".into(),
            state: "FATAL".into(),
            pid: None,
            uptime: None,
            command: None,
            directory: None,
            user: None,
            role: ProcessRole::Queue,
        });
        snap.certificates.push(Certificate {
            path: "/etc/ssl/a.pem".into(),
            subject: None,
            issuer: None,
            names: vec![],
            not_after: Some("2026-11-05T00:00:00Z".into()),
        });

        let list = insights(&snap, datetime!(2026-10-26 0:00 UTC));
        let msgs: Vec<&str> = list.iter().map(|i| i.message.as_str()).collect();
        assert_eq!(list[0].level, Level::Critical);
        assert!(msgs[0].contains("worker is FATAL"));
        assert!(msgs.iter().any(|m| m.contains("ufw")));
        assert!(msgs.iter().any(|m| m.contains("Port 8000")));
        assert!(!msgs.iter().any(|m| m.contains("Port 443")));
        assert!(msgs.iter().any(|m| m.contains("expires in 10 days")));
        assert!(msgs.iter().any(|m| m.contains("FPM is running but no site uses it")));
        let fpm = list.iter().find(|i| i.code == "fpm_unused").unwrap();
        assert_eq!(fpm.params, vec![("version", "8.5.11".to_string())]);
    }
}
