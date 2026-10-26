//! End-to-end discovery against recorded output of a FrankenPHP + Octane
//! server (the stack Sesame is modelled on).

use std::sync::Mutex;

use anyhow::Result;
use sesame_core::snapshot::*;
use sesame_ssh::{CommandOutput, Exec};

use crate::{discover, script};

struct Recorded {
    probe: &'static str,
    details: &'static str,
    commands: Mutex<Vec<String>>,
}

impl Exec for Recorded {
    async fn exec(&self, command: &str) -> Result<CommandOutput> {
        self.commands.lock().unwrap().push(command.to_string());
        let stdout = if command == script::PROBE { self.probe } else { self.details };
        Ok(CommandOutput { stdout: stdout.into(), stderr: String::new(), exit_code: Some(0) })
    }
}

fn frankenphp_server() -> Recorded {
    Recorded {
        probe: include_str!("../fixtures/frankenphp-octane.probe"),
        details: include_str!("../fixtures/frankenphp-octane.details"),
        commands: Mutex::new(Vec::new()),
    }
}

#[tokio::test]
async fn discovers_frankenphp_octane_server() {
    let server = frankenphp_server();
    let snap = discover(&server).await.unwrap();

    assert_eq!(snap.os.distro, "Ubuntu 24.04.5 LTS");
    assert_eq!(snap.os.arch, "aarch64");
    assert_eq!(snap.os.cpus, 2);
    assert!(snap.os.privileged);
    assert!(snap.warnings.is_empty(), "{:?}", snap.warnings);

    assert_eq!(snap.web_servers[0].kind, WebServerKind::FrankenPhp);
    assert_eq!(
        snap.web_servers[0].version.as_deref(),
        Some("FrankenPHP v1.13.1 PHP 8.5 Caddy v2.11.7")
    );

    let site = &snap.sites[0];
    assert_eq!(site.domains, vec!["new.example.com", "example.com"]);
    assert_eq!(site.app_path.as_deref(), Some("/var/www/shop"));
    assert_eq!(site.framework, Framework::Laravel);
    assert_eq!(site.framework_version.as_deref(), Some("12.31.1"));
    assert_eq!(site.env.as_ref().unwrap().db_database.as_deref(), Some("shop"));
    assert_eq!(site.processes.len(), 4, "octane, 2 queue workers, reverb");

    let roles: Vec<ProcessRole> = snap.supervisor.iter().map(|p| p.role).collect();
    assert_eq!(roles, [ProcessRole::Octane, ProcessRole::Queue, ProcessRole::Queue, ProcessRole::Reverb]);

    let kinds: Vec<DatabaseKind> = snap.databases.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [DatabaseKind::MySql, DatabaseKind::Redis]);
    assert_eq!(snap.databases[0].databases, vec!["shop"]);

    assert!(snap.mail.servers.is_empty());
    assert_eq!(snap.mail.external, vec!["smtp via smtp.resend.com"]);

    assert_eq!(snap.php[0].version, "8.5.11");
    assert_eq!(snap.cron.len(), 2);
    assert_eq!(snap.firewall.ufw.as_deref(), Some("inactive"));
    assert_eq!(snap.firewall.fail2ban_jails, vec!["sshd"]);

    let cert = &snap.certificates[0];
    assert_eq!(cert.path, "/etc/ssl/shop/origin.pem");
    assert_eq!(cert.not_after.as_deref(), Some("2041-10-10T09:21:00Z"));

    let public: Vec<u16> = snap.ports.iter().filter(|p| p.public).map(|p| p.port).collect();
    assert_eq!(public, vec![22, 80, 443, 8000]);
}

#[tokio::test]
async fn details_phase_skips_stock_pages_and_reads_no_secrets() {
    let server = frankenphp_server();
    discover(&server).await.unwrap();
    let commands = server.commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    let details = &commands[1];
    assert!(details.contains("'/var/www/shop'/artisan"));
    assert!(!details.contains("/usr/share/frankenphp"));
    assert!(!details.contains("PASSWORD"));
    assert!(!details.contains("APP_KEY"));
}
