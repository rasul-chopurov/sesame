//! The tool list with JSON schemas, as agents see it.

use serde_json::{Value, json};
use sesame_core::ops::Risk;

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: Risk,
    /// Properties of the input object and which are required.
    pub params: &'static [(&'static str, &'static str, &'static str, bool)],
}

const SERVER: (&str, &str, &str, bool) = ("server", "string", "Server name as shown by list_servers", true);

pub const TOOLS: &[Tool] = &[
    Tool { name: "list_servers", description: "List the servers added to Sesame with their last discovery time.", risk: Risk::Read, params: &[] },
    Tool { name: "discover_server", description: "Connect over SSH and discover everything on a server, read-only (sites, databases, processes, ports, certificates, firewall). Saves and returns a summary.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "get_server", description: "Summary of the last discovery of a server, including insights that need attention.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "get_snapshot", description: "The full last discovery snapshot of a server as JSON.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "list_sites", description: "Sites on a server with framework, directory, non-secret .env values, git checkout and processes.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "list_databases", description: "Database engines on a server, their databases and sizes.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "list_services", description: "Which installable services and admin panels are installed on a server.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "get_insights", description: "Problems worth attention on a server (failed processes, open ports, expiring certificates, …).", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "audit_log", description: "Recent changes Sesame made, newest first.", risk: Risk::Read, params: &[("server", "string", "Only this server", false), ("limit", "integer", "How many entries (default 20)", false)] },
    Tool { name: "cloudflare_zones", description: "Cloudflare domains (zones) of the connected account.", risk: Risk::Read, params: &[] },
    Tool { name: "cloudflare_records", description: "DNS records of a Cloudflare zone.", risk: Risk::Read, params: &[("zone", "string", "Zone name, e.g. example.com", true)] },
    Tool { name: "prepare_server", description: "Prepare a server for Laravel from zero, or complete a half-set-up one: base packages, security updates, swap, PHP with extensions, Composer, Caddy or nginx, supervisor, fail2ban, ufw. Installs only what is missing. Needs approval.", risk: Risk::Change, params: &[SERVER, ("web", "string", "caddy (default) or nginx, when there is no web server yet", false), ("php", "string", "PHP version when there is no PHP yet (default 8.4)", false), ("firewall", "boolean", "Turn on ufw with SSH, 80 and 443 (default true)", false)] },
    Tool { name: "service_action", description: "Start, stop, restart, enable or disable a systemd service or supervisor program. Needs the user's approval in Sesame.", risk: Risk::Change, params: &[SERVER, ("kind", "string", "systemd or supervisor", true), ("name", "string", "Unit or program name", true), ("action", "string", "start, stop, restart, enable or disable", true)] },
    Tool { name: "install_service", description: "Install redis, postgresql, mariadb, mongodb, elasticsearch, rustfs (S3), phpmyadmin or pgadmin with localhost-only defaults. Needs approval.", risk: Risk::Change, params: &[SERVER, ("service", "string", "redis | postgresql | mariadb | mongodb | elasticsearch | rustfs | phpmyadmin | pgadmin", true)] },
    Tool { name: "uninstall_service", description: "Uninstall a service; data is kept unless delete_data is true. Needs approval.", risk: Risk::Destructive, params: &[SERVER, ("service", "string", "Service slug as in install_service", true), ("delete_data", "boolean", "Also delete its data (default false)", false)] },
    Tool { name: "create_database", description: "Create a database and a user limited to it. The password is stored in Sesame's vault. Needs approval.", risk: Risk::Change, params: &[SERVER, ("engine", "string", "mysql | mariadb | postgres | mongodb", true), ("name", "string", "Database name", true), ("user", "string", "User name (default: same as name)", false)] },
    Tool { name: "drop_database", description: "Back up a database to /var/backups/sesame, then drop it. Needs approval.", risk: Risk::Destructive, params: &[SERVER, ("engine", "string", "mysql | mariadb | postgres | mongodb", true), ("name", "string", "Database name", true), ("user", "string", "Also drop this user", false)] },
    Tool { name: "create_site", description: "Create an isolated site: own Linux user, PHP-FPM pool or Octane, web server file, optional database, Redis ACL, queue worker and scheduler. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Domain, e.g. shop.example.com", true), ("kind", "string", "laravel (default) or static", false), ("runtime", "string", "fpm (default) or octane", false), ("repo", "string", "Public GitHub repository owner/name to deploy", false), ("branch", "string", "Branch (default main)", false), ("web_root", "string", "Web root inside the app (default public or .)", false), ("database", "string", "mysql | mariadb | postgres", false), ("redis", "boolean", "Give the site its own Redis ACL user", false), ("queue", "boolean", "Run a queue worker", false), ("scheduler", "boolean", "Laravel scheduler (default true)", false)] },
    Tool { name: "remove_site", description: "Delete a site created by Sesame; its database is backed up first. Needs approval.", risk: Risk::Destructive, params: &[SERVER, ("domain", "string", "Site domain", true), ("database", "string", "Engine of the site's database to drop", false)] },
    Tool { name: "site_maintenance", description: "Laravel maintenance mode on or off for a site. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("on", "boolean", "true = down, false = up", true)] },
    Tool { name: "site_database", description: "Create a database for an existing Laravel site, write DB_* to its .env and migrate. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("engine", "string", "mysql | mariadb | postgres", true)] },
    Tool { name: "site_redis", description: "Give a site its own Redis ACL user limited to its key prefix and write it to .env. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true)] },
    Tool { name: "site_queue", description: "Run a queue worker for a site under supervisor as the site's user. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("connection", "string", "Queue connection (default redis)", false)] },
    Tool { name: "site_scheduler", description: "Add the Laravel scheduler to the site user's crontab. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true)] },
    Tool { name: "deploy_site", description: "Deploy the latest commit of the site's linked GitHub branch (composer, migrations, caches). Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true)] },
    Tool { name: "cloudflare_upsert_record", description: "Create or update a DNS record in Cloudflare. Needs approval.", risk: Risk::Change, params: &[("zone", "string", "Zone name", true), ("type", "string", "A | AAAA | CNAME | TXT | MX | NS | CAA", true), ("name", "string", "Full record name, e.g. www.example.com", true), ("content", "string", "Value", true), ("proxied", "boolean", "Through Cloudflare's proxy (A/AAAA/CNAME)", false), ("ttl", "integer", "Seconds, 1 = auto", false), ("priority", "integer", "MX priority", false)] },
    Tool { name: "cloudflare_delete_record", description: "Delete a DNS record in Cloudflare. Needs approval.", risk: Risk::Destructive, params: &[("zone", "string", "Zone name", true), ("name", "string", "Full record name", true), ("type", "string", "Record type, when a name has several", false)] },
    Tool { name: "cloudflare_purge", description: "Purge Cloudflare's cache for a zone. Needs approval.", risk: Risk::Change, params: &[("zone", "string", "Zone name", true)] },
    Tool { name: "mail_domains", description: "Domains in the connected Resend account with their DNS records and verification status.", risk: Risk::Read, params: &[] },
    Tool { name: "site_mail_resend", description: "Send a site's mail through Resend: add its domain to Resend, publish DKIM/SPF in Cloudflare, verify, and for Laravel write a domain-limited key as MAIL_* to .env. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true)] },
    Tool { name: "site_enabled", description: "Turn a site created by Sesame on or off (its web-server file is set aside; files, database and workers stay). Needs approval.", risk: Risk::Destructive, params: &[SERVER, ("domain", "string", "Site domain", true), ("enabled", "boolean", "true = on, false = off", true)] },
    Tool { name: "site_access_grant", description: "Give a person their own SSH key access to a site's owner account: SFTP only (default) or a shell. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("person", "string", "Short name, e.g. anna", true), ("public_key", "string", "OpenSSH public key line", true), ("shell", "boolean", "Allow a shell, not just SFTP (default false)", false)] },
    Tool { name: "site_access_revoke", description: "Remove a person's key from a site's owner account. Needs approval.", risk: Risk::Destructive, params: &[SERVER, ("domain", "string", "Site domain", true), ("person", "string", "Name used when access was granted", true)] },
    // ---- servers and integrations ----
    Tool { name: "add_server", description: "Save a server: user@host[:port]. Authenticates with the ssh-agent or keys of the machine Sesame runs on, a key file, or a password (kept in the vault).", risk: Risk::Change, params: &[("name", "string", "Short name, e.g. prod", true), ("target", "string", "user@host[:port]", true), ("key_path", "string", "Private key file instead of the agent", false), ("password", "string", "SSH password instead of keys", false)] },
    Tool { name: "remove_server", description: "Remove a server from Sesame with its saved secrets. The server itself is not touched.", risk: Risk::Destructive, params: &[SERVER] },
    Tool { name: "integrations", description: "Which integrations are connected (GitHub, Cloudflare, Resend) and as whom.", risk: Risk::Read, params: &[] },
    Tool { name: "set_integration", description: "Save or replace an integration token in the vault after checking it works.", risk: Risk::Change, params: &[("kind", "string", "github | cloudflare | resend", true), ("token", "string", "API token", true)] },
    Tool { name: "remove_integration", description: "Forget an integration token.", risk: Risk::Destructive, params: &[("kind", "string", "github | cloudflare | resend", true)] },
    // ---- GitHub ----
    Tool { name: "github_repos", description: "Repositories the connected GitHub token can see.", risk: Risk::Read, params: &[] },
    Tool { name: "github_branches", description: "Branches of a GitHub repository.", risk: Risk::Read, params: &[("repo", "string", "owner/name", true)] },
    Tool { name: "github_head", description: "Latest commit of a branch on GitHub.", risk: Risk::Read, params: &[("repo", "string", "owner/name", true), ("branch", "string", "Branch", true)] },
    Tool { name: "site_links", description: "GitHub repository and branch linked to each site of a server.", risk: Risk::Read, params: &[SERVER] },
    Tool { name: "github_link_site", description: "Link a site to a GitHub repository and branch: a read-only deploy key is made on the server and added to the repository. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("repo", "string", "owner/name", true), ("branch", "string", "Branch", true)] },
    // ---- Cloudflare, Reverb ----
    Tool { name: "cloudflare_accounts", description: "Cloudflare accounts of the connected token.", risk: Risk::Read, params: &[] },
    Tool { name: "cloudflare_create_zone", description: "Add a domain to Cloudflare; returns the nameservers to set at the registrar.", risk: Risk::Change, params: &[("name", "string", "Domain, e.g. example.com", true), ("account_id", "string", "Account (default: the first)", false)] },
    Tool { name: "site_reverb", description: "Run Laravel Reverb (WebSockets) for a site under supervisor and route /app through the web server. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("port", "integer", "Local port (default: a free one from 8080)", false)] },
    // ---- files of a site (SFTP, confined to the site directory) ----
    Tool { name: "files_list", description: "List a directory inside a site's folder.", risk: Risk::Read, params: &[SERVER, ("domain", "string", "Site domain", true), ("path", "string", "Path inside the site, e.g. / or app/Models", false)] },
    Tool { name: "files_read", description: "Read a text file inside a site's folder (up to 1 MB).", risk: Risk::Read, params: &[SERVER, ("domain", "string", "Site domain", true), ("path", "string", "File path inside the site", true)] },
    Tool { name: "files_write", description: "Write a text file inside a site's folder; it is owned by the site's user. Needs approval.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("path", "string", "File path inside the site", true), ("content", "string", "New content", true)] },
    Tool { name: "files_mkdir", description: "Create a folder inside a site's folder.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("path", "string", "Folder path inside the site", true)] },
    Tool { name: "files_rename", description: "Rename or move inside a site's folder.", risk: Risk::Change, params: &[SERVER, ("domain", "string", "Site domain", true), ("from", "string", "Current path", true), ("to", "string", "New path", true)] },
    Tool { name: "files_remove", description: "Delete a file or an empty folder inside a site's folder.", risk: Risk::Destructive, params: &[SERVER, ("domain", "string", "Site domain", true), ("path", "string", "Path inside the site", true)] },
    Tool { name: "run_command", description: "Run a shell command on a server as root and return its output. Always needs approval; prefer the specific tools.", risk: Risk::Destructive, params: &[SERVER, ("command", "string", "POSIX shell command", true)] },
];

pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

impl Tool {
    pub fn input_schema(&self) -> Value {
        let mut props = serde_json::Map::new();
        let mut required = Vec::new();
        for (name, ty, desc, req) in self.params {
            props.insert((*name).into(), json!({ "type": ty, "description": desc }));
            if *req {
                required.push(*name);
            }
        }
        json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_known_and_has_a_schema() {
        let mut names = std::collections::HashSet::new();
        for t in TOOLS {
            assert!(names.insert(t.name), "duplicate {}", t.name);
            let s = t.input_schema();
            assert_eq!(s["type"], "object");
            let global = t.name.starts_with("cloudflare_") || matches!(t.name, "add_server" | "set_integration" | "remove_integration");
            if t.risk != Risk::Read && !global {
                assert!(t.params.iter().any(|p| p.0 == "server"), "{} needs a server", t.name);
            }
        }
        assert_eq!(find("run_command").unwrap().risk, Risk::Destructive);
    }
}
