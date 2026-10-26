//! Service recipes for Ubuntu 22.04/24.04 and Debian 12 (amd64 and arm64).
//!
//! Every recipe binds to localhost, generates its own password (shown once,
//! kept in the encrypted vault by the app) and never opens a port to the internet.
//! Uninstalling keeps data unless the user explicitly asks to delete it.
//!
//! Scripts run as root through `sesame_ops::execute` with `set -eu`.

use anyhow::{Result, bail};
use sesame_core::ops::Risk;
use sesame_core::snapshot::{DatabaseKind, Framework, ServerSnapshot, Site};
use sesame_ops::actions::q;
use sesame_ops::{Plan, Step, generate_password};

pub mod stack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Redis,
    Postgres,
    MariaDb,
    MongoDb,
    Elasticsearch,
    RustFs,
    Stalwart,
}

/// Stalwart release Sesame installs, with the SHA-256 of each build
/// (from the GitHub release's asset digests).
const STALWART_VERSION: &str = "v0.16.27";
const STALWART_SHA256: [(&str, &str); 2] = [
    ("x86_64", "3d4548053e7130cdc6288134e28aecc5004f22549cc52a1dbda0ca48ff0b301a"),
    ("aarch64", "215301c8449e3bf8efd4c39cd557dc65dc37ec5bedab47f15c22ef24631100d7"),
];

impl Service {
    pub const ALL: [Service; 7] = [
        Service::Redis,
        Service::Postgres,
        Service::MariaDb,
        Service::MongoDb,
        Service::Elasticsearch,
        Service::RustFs,
        Service::Stalwart,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Service::Redis => "Redis",
            Service::Postgres => "PostgreSQL",
            Service::MariaDb => "MariaDB",
            Service::MongoDb => "MongoDB",
            Service::Elasticsearch => "Elasticsearch",
            Service::RustFs => "RustFS",
            Service::Stalwart => "Stalwart Mail",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Service::Redis => "redis",
            Service::Postgres => "postgresql",
            Service::MariaDb => "mariadb",
            Service::MongoDb => "mongodb",
            Service::Elasticsearch => "elasticsearch",
            Service::RustFs => "rustfs",
            Service::Stalwart => "stalwart",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub installed: bool,
    pub version: Option<String>,
    /// Something already on the server that rules this out.
    pub conflict: Option<String>,
}

fn engine(s: &ServerSnapshot, kind: DatabaseKind) -> Option<String> {
    s.databases.iter().find(|d| d.kind == kind).map(|d| d.version.clone().unwrap_or_default())
}

fn has_unit(s: &ServerSnapshot, prefix: &str) -> bool {
    s.services.iter().any(|u| u.unit.starts_with(prefix))
}

pub fn detect(service: Service, s: &ServerSnapshot) -> Status {
    let found = match service {
        Service::Redis => engine(s, DatabaseKind::Redis),
        Service::Postgres => engine(s, DatabaseKind::Postgres),
        Service::MariaDb => engine(s, DatabaseKind::MariaDb),
        Service::MongoDb => engine(s, DatabaseKind::MongoDb),
        Service::Elasticsearch => {
            (has_unit(s, "elasticsearch") || s.ports.iter().any(|p| p.port == 9200)).then(String::new)
        }
        Service::RustFs => (has_unit(s, "rustfs") || s.ports.iter().any(|p| p.process == "rustfs")).then(String::new),
        Service::Stalwart => (has_unit(s, "stalwart") || s.ports.iter().any(|p| p.process == "stalwart")).then(String::new),
    };
    let conflict = match service {
        Service::MariaDb if found.is_none() && engine(s, DatabaseKind::MySql).is_some() => Some("MySQL".to_string()),
        Service::RustFs if found.is_none() && s.ports.iter().any(|p| p.port == 9000) => Some("port 9000".to_string()),
        Service::Stalwart if found.is_none() => {
            let mta = ["postfix", "exim4", "sendmail"].into_iter().find(|m| has_unit(s, m));
            match mta {
                Some(m) => Some(m.to_string()),
                None => s.ports.iter().find(|p| [25, 465, 587, 993, 8080].contains(&p.port)).map(|p| format!("port {}", p.port)),
            }
        }
        _ => None,
    };
    Status { installed: found.is_some(), version: found.filter(|v| !v.is_empty()), conflict }
}

pub(crate) const APT: &str = "apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q";

pub(crate) fn preflight() -> String {
    r#"case "$ID" in ubuntu|debian) ;; *) echo "Sesame recipes support Ubuntu and Debian; this is $ID"; exit 1;; esac
command -v systemctl >/dev/null || { echo "systemd is required"; exit 1; }"#
        .to_string()
}

fn apt(packages: &str) -> String {
    format!("{APT} update >/dev/null\n{APT} install {packages}")
}

pub fn install(service: Service, s: &ServerSnapshot) -> Result<Plan> {
    let status = detect(service, s);
    if status.installed {
        bail!("{} is already installed", service.name());
    }
    if let Some(c) = status.conflict {
        bail!("{} conflicts with {c} on this server", service.name());
    }
    let pw = generate_password();
    let mut reveal = Vec::new();
    let mut details = vec!["listens on 127.0.0.1 only".to_string()];
    let mut steps = vec![Step::new("check the system", preflight())];

    match service {
        Service::Redis => {
            steps.push(Step::new("install redis-server", apt("redis-server")));
            steps.push(Step::new(
                "require a password, bind to localhost",
                format!(
                    "cat > /etc/redis/sesame.conf <<'CONF'\nbind 127.0.0.1\nprotected-mode yes\nrequirepass {pw}\nCONF\n\
                     chown redis:redis /etc/redis/sesame.conf && chmod 640 /etc/redis/sesame.conf\n\
                     grep -q '^include /etc/redis/sesame.conf' /etc/redis/redis.conf || echo 'include /etc/redis/sesame.conf' >> /etc/redis/redis.conf\n\
                     systemctl enable redis-server\nsystemctl restart redis-server\nsystemctl is-active redis-server"
                ),
            ));
            details.push("password required (REDIS_PASSWORD)".into());
            reveal.push(("REDIS_PASSWORD".into(), pw));
        }
        Service::Postgres => {
            steps.push(Step::new(
                "add the PostgreSQL apt repository",
                format!(
                    "{}\ninstall -d /usr/share/postgresql-common/pgdg\n\
                     curl -fsSL -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc https://www.postgresql.org/media/keys/ACCC4CF8.asc\n\
                     echo \"deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt $VERSION_CODENAME-pgdg main\" > /etc/apt/sources.list.d/pgdg.list",
                    apt("curl ca-certificates")
                ),
            ));
            steps.push(Step::new("install postgresql", format!("{}\nsystemctl enable --now postgresql", apt("postgresql"))));
            steps.push(Step::new("check", "cd /tmp && runuser -u postgres -- psql -Atc 'select version()'"));
            details.push("latest stable PostgreSQL from apt.postgresql.org; local users authenticate as themselves".into());
        }
        Service::MariaDb => {
            steps.push(Step::new(
                "install mariadb-server",
                format!(
                    "if dpkg -s mysql-server >/dev/null 2>&1; then echo 'MySQL is installed; MariaDB would replace it'; exit 1; fi\n{}\nsystemctl enable --now mariadb",
                    apt("mariadb-server")
                ),
            ));
            steps.push(Step::new("check", "mysql -Nse 'select version()'"));
            details.push("root signs in through the local socket only".into());
        }
        Service::MongoDb => {
            steps.push(Step::new(
                "add the MongoDB 8.0 apt repository",
                format!(
                    "case \"$ID:$VERSION_CODENAME\" in\n\
                     \x20 ubuntu:jammy|ubuntu:noble) REPO=\"https://repo.mongodb.org/apt/ubuntu $VERSION_CODENAME/mongodb-org/8.0 multiverse\";;\n\
                     \x20 debian:bookworm) REPO=\"https://repo.mongodb.org/apt/debian bookworm/mongodb-org/8.0 main\";;\n\
                     \x20 *) echo \"MongoDB 8.0 does not support $ID $VERSION_CODENAME\"; exit 1;;\n\
                     esac\n{}\n\
                     curl -fsSL https://www.mongodb.org/static/pgp/server-8.0.asc | gpg --dearmor --yes -o /usr/share/keyrings/mongodb-server-8.0.gpg\n\
                     echo \"deb [signed-by=/usr/share/keyrings/mongodb-server-8.0.gpg] $REPO\" > /etc/apt/sources.list.d/mongodb-org-8.0.list",
                    apt("curl gnupg ca-certificates")
                ),
            ));
            steps.push(Step::new(
                "install mongodb-org",
                format!(
                    "{}\nsystemctl enable --now mongod\n\
                     for i in $(seq 1 30); do mongosh --quiet --eval 'db.runCommand({{ping:1}}).ok' >/dev/null 2>&1 && break; sleep 1; done",
                    apt("mongodb-org")
                ),
            ));
            // On data kept from an earlier install, auth is already on and the old
            // admin stays; only a fresh install creates one and reveals its password.
            steps.push(
                Step::new(
                    "create the admin user",
                    format!(
                        "if grep -q '^security:' /etc/mongod.conf; then exit 0; fi\n{} >/dev/null\nprintf '%s' '{pw}'",
                        sesame_ops::actions::mongo_js(
                            None,
                            &format!("db.getSiblingDB('admin').createUser({{user: 'sesame_admin', pwd: '{pw}', roles: ['root']}});")
                        )
                    ),
                )
                .capture("MONGO_ADMIN_PASSWORD"),
            );
            steps.push(Step::new(
                "turn on authentication",
                "grep -q '^security:' /etc/mongod.conf || printf '\\nsecurity:\\n  authorization: enabled\\n' >> /etc/mongod.conf\n\
                 systemctl restart mongod\nsystemctl is-active mongod",
            ));
            details.push("authentication on; admin user sesame_admin (kept if data from an earlier install exists)".into());
            reveal.push(("MONGO_ADMIN_USER".into(), "sesame_admin".into()));
        }
        Service::Elasticsearch => {
            steps.push(Step::new(
                "check memory",
                "avail=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)\n\
                 if [ \"$avail\" -lt 2048 ]; then echo \"Elasticsearch needs 2 GB of free memory; ${avail} MB is available\"; exit 1; fi\n\
                 echo \"${avail} MB available\"",
            ));
            steps.push(Step::new(
                "add the Elastic apt repository",
                format!(
                    "{}\ncurl -fsSL https://artifacts.elastic.co/GPG-KEY-elasticsearch | gpg --dearmor --yes -o /usr/share/keyrings/elasticsearch-keyring.gpg\n\
                     echo 'deb [signed-by=/usr/share/keyrings/elasticsearch-keyring.gpg] https://artifacts.elastic.co/packages/9.x/apt stable main' > /etc/apt/sources.list.d/elastic-9.x.list",
                    apt("curl gnupg ca-certificates")
                ),
            ));
            steps.push(Step::new(
                "install elasticsearch",
                // The package prints a generated password; keep it out of the log.
                format!(
                    "{APT} update >/dev/null\n{APT} install elasticsearch >/dev/null 2>&1\n\
                     total=$(awk '/MemTotal/ {{print int($2/1024)}}' /proc/meminfo); heap=$((total / 4))\n\
                     mkdir -p /etc/elasticsearch/jvm.options.d\n\
                     printf -- '-Xms%sm\\n-Xmx%sm\\n' \"$heap\" \"$heap\" > /etc/elasticsearch/jvm.options.d/sesame.options\n\
                     y=/etc/elasticsearch/elasticsearch.yml\n\
                     # The package's security auto-config writes http.host: 0.0.0.0; pin everything to localhost.\n\
                     sed -i -E '/^(network|http|transport)\\.host:/d' \"$y\"\n\
                     printf 'network.host: 127.0.0.1\\nhttp.host: 127.0.0.1\\ntransport.host: 127.0.0.1\\n' >> \"$y\"\n\
                     systemctl daemon-reload\nsystemctl enable --now elasticsearch\necho \"heap ${{heap}} MB\""
                ),
            ));
            steps.push(
                Step::new(
                    "set the elastic user's password",
                    "/usr/share/elasticsearch/bin/elasticsearch-reset-password -u elastic -b -s",
                )
                .capture("ELASTIC_PASSWORD"),
            );
            details.push("heap = ¼ of RAM; security on; user elastic".into());
        }
        Service::RustFs => {
            steps.push(Step::new(
                "download the latest RustFS release",
                format!(
                    "case \"$(uname -m)\" in x86_64) A=x86_64;; aarch64) A=aarch64;; *) echo \"unsupported CPU $(uname -m)\"; exit 1;; esac\n{}\n\
                     V=$(curl -fsSL https://api.github.com/repos/rustfs/rustfs/releases/latest | sed -n 's/.*\"tag_name\": *\"\\([^\"]*\\)\".*/\\1/p' | head -1)\n\
                     [ -n \"$V\" ] || {{ echo 'could not find the latest RustFS release'; exit 1; }}\n\
                     tmp=$(mktemp -d)\n\
                     curl -fsSL -o \"$tmp/rustfs.zip\" \"https://github.com/rustfs/rustfs/releases/download/$V/rustfs-linux-$A-gnu-v$V.zip\"\n\
                     unzip -o -q \"$tmp/rustfs.zip\" -d \"$tmp\"\n\
                     install -m 755 \"$(find \"$tmp\" -type f -name rustfs | head -1)\" /usr/local/bin/rustfs\n\
                     rm -rf \"$tmp\"\necho \"RustFS $V\"",
                    apt("curl unzip ca-certificates")
                ),
            ));
            steps.push(Step::new(
                "create the service",
                format!(
                    "id rustfs >/dev/null 2>&1 || useradd --system --home /var/lib/rustfs --shell /usr/sbin/nologin rustfs\n\
                     install -d -o rustfs -g rustfs -m 750 /var/lib/rustfs /var/lib/rustfs/data\n\
                     install -d -m 750 /etc/rustfs\n\
                     cat > /etc/rustfs/rustfs.env <<'ENV'\n\
                     RUSTFS_ACCESS_KEY=sesame\n\
                     RUSTFS_SECRET_KEY={pw}\n\
                     RUSTFS_VOLUMES=/var/lib/rustfs/data\n\
                     RUSTFS_ADDRESS=127.0.0.1:9000\n\
                     RUSTFS_CONSOLE_ENABLE=true\n\
                     RUSTFS_CONSOLE_ADDRESS=127.0.0.1:9001\n\
                     ENV\n\
                     chown root:rustfs /etc/rustfs/rustfs.env && chmod 640 /etc/rustfs/rustfs.env\n\
                     cat > /etc/systemd/system/rustfs.service <<'UNIT'\n\
                     [Unit]\nDescription=RustFS S3-compatible object storage\nAfter=network-online.target\nWants=network-online.target\n\n\
                     [Service]\nUser=rustfs\nGroup=rustfs\nEnvironmentFile=/etc/rustfs/rustfs.env\nExecStart=/usr/local/bin/rustfs $RUSTFS_VOLUMES\n\
                     Restart=always\nLimitNOFILE=1048576\n\n[Install]\nWantedBy=multi-user.target\nUNIT\n\
                     systemctl daemon-reload\nsystemctl enable --now rustfs\nsleep 2\nsystemctl is-active rustfs"
                ),
            ));
            details.push("S3 API on 127.0.0.1:9000, console on 127.0.0.1:9001".into());
            reveal.push(("AWS_ACCESS_KEY_ID".into(), "sesame".into()));
            reveal.push(("AWS_SECRET_ACCESS_KEY".into(), pw));
            reveal.push(("AWS_ENDPOINT".into(), "http://127.0.0.1:9000".into()));
            reveal.push(("AWS_USE_PATH_STYLE_ENDPOINT".into(), "true".into()));
        }
        Service::Stalwart => {
            let sums = STALWART_SHA256.iter().map(|(a, h)| format!("{a}) SUM={h};;")).collect::<Vec<_>>().join(" ");
            steps.push(Step::new(
                format!("download Stalwart {STALWART_VERSION} and check its SHA-256"),
                format!(
                    "{}\nA=$(uname -m)\ncase \"$A\" in {sums} *) echo \"unsupported CPU $A\"; exit 1;; esac\n\
                     tmp=$(mktemp -d)\n\
                     curl -fsSL -o \"$tmp/s.tgz\" \"https://github.com/stalwartlabs/stalwart/releases/download/{STALWART_VERSION}/stalwart-$A-unknown-linux-gnu.tar.gz\"\n\
                     echo \"$SUM  $tmp/s.tgz\" | sha256sum -c --quiet || {{ echo 'checksum mismatch — refusing to install'; rm -rf \"$tmp\"; exit 1; }}\n\
                     tar -xzf \"$tmp/s.tgz\" -C \"$tmp\"\n\
                     install -m 755 \"$tmp/stalwart\" /usr/local/bin/stalwart\n\
                     rm -rf \"$tmp\"\necho 'Stalwart {STALWART_VERSION} verified'",
                    apt("curl ca-certificates")
                ),
            ));
            steps.push(Step::new(
                "create the service",
                format!(
                    "id stalwart >/dev/null 2>&1 || useradd stalwart -s /usr/sbin/nologin -M -r -U\n\
                     install -d -o stalwart -g stalwart -m 750 /etc/stalwart /var/lib/stalwart /var/log/stalwart\n\
                     if [ ! -e /etc/stalwart/stalwart.env ]; then\n\
                     cat > /etc/stalwart/stalwart.env <<'ENV'\n\
                     STALWART_RECOVERY_ADMIN=admin:{pw}\n\
                     ENV\n\
                     fi\n\
                     chown root:stalwart /etc/stalwart/stalwart.env && chmod 640 /etc/stalwart/stalwart.env\n\
                     cat > /etc/systemd/system/stalwart.service <<'UNIT'\n\
                     [Unit]\nDescription=Stalwart\nConflicts=postfix.service sendmail.service exim4.service\nAfter=network-online.target\n\n\
                     [Service]\nType=simple\nLimitNOFILE=65536\nKillMode=process\nKillSignal=SIGINT\nRestart=on-failure\nRestartSec=5\n\
                     EnvironmentFile=-/etc/stalwart/stalwart.env\nExecStart=/usr/local/bin/stalwart --config=/etc/stalwart/config.json\n\
                     SyslogIdentifier=stalwart\nUser=stalwart\nGroup=stalwart\nAmbientCapabilities=CAP_NET_BIND_SERVICE\n\n\
                     [Install]\nWantedBy=multi-user.target\nUNIT\n\
                     systemctl daemon-reload\nsystemctl enable --now stalwart\nsleep 3\nsystemctl is-active stalwart"
                ),
            ));
            details.push(format!("Stalwart {STALWART_VERSION}: SMTP, IMAP and JMAP in one Rust binary, checksum-verified"));
            details.push("admin user `admin`, password generated and kept in the vault".into());
            details.push("its setup page is on port 8080 — keep 8080 closed in your provider's firewall; open it through an SSH tunnel".into());
            details.push("many providers (Hetzner included) block outgoing port 25 on new servers: relay outgoing mail through Resend".into());
            reveal.push(("STALWART_ADMIN_USER".into(), "admin".into()));
            reveal.push(("STALWART_ADMIN_PASSWORD".into(), pw));
        }
    }
    Ok(Plan {
        op: "service_install",
        title: format!("install {}", service.name()),
        risk: Risk::Change,
        details,
        confirm_word: None,
        steps,
        reveal,
        secret_scope: None,
    })
}

pub fn uninstall(service: Service, keep_data: bool) -> Result<Plan> {
    let (unit, packages, data, extra) = match service {
        Service::Redis => ("redis-server", "redis-server", "/var/lib/redis", "rm -f /etc/redis/sesame.conf"),
        Service::Postgres => ("postgresql", "'postgresql*'", "/var/lib/postgresql", "rm -f /etc/apt/sources.list.d/pgdg.list"),
        Service::MariaDb => ("mariadb", "'mariadb-server*'", "/var/lib/mysql", ""),
        Service::MongoDb => ("mongod", "'mongodb-org*'", "/var/lib/mongodb", "rm -f /etc/apt/sources.list.d/mongodb-org-8.0.list"),
        Service::Elasticsearch => ("elasticsearch", "elasticsearch", "/var/lib/elasticsearch", "rm -f /etc/apt/sources.list.d/elastic-9.x.list"),
        Service::RustFs => ("rustfs", "", "/var/lib/rustfs", ""),
        Service::Stalwart => ("stalwart", "", "/var/lib/stalwart", ""),
    };
    let mut steps = vec![Step::new(
        format!("stop {unit}"),
        format!("systemctl disable --now {unit} 2>/dev/null || true\necho stopped"),
    )];
    if service == Service::Stalwart {
        steps.push(Step::new(
            "remove Stalwart",
            "rm -f /etc/systemd/system/stalwart.service /usr/local/bin/stalwart\nsystemctl daemon-reload\necho removed",
        ));
    } else if service == Service::RustFs {
        steps.push(Step::new(
            "remove RustFS",
            "rm -f /etc/systemd/system/rustfs.service /usr/local/bin/rustfs\nsystemctl daemon-reload\nrm -rf /etc/rustfs",
        ));
    } else {
        let verb = if keep_data { "remove" } else { "purge" };
        steps.push(Step::new(
            format!("{verb} packages"),
            format!("{APT} {verb} {packages}\n{APT} autoremove >/dev/null\n{extra}"),
        ));
    }
    if !keep_data {
        steps.push(Step::new(format!("delete {data}"), format!("rm -rf {}\necho deleted", q(data))));
        if service == Service::RustFs {
            steps.push(Step::new("remove the rustfs user", "userdel rustfs 2>/dev/null || true"));
        }
        if service == Service::Stalwart {
            steps.push(Step::new("remove Stalwart's settings and user", "rm -rf /etc/stalwart /var/log/stalwart\nuserdel stalwart 2>/dev/null || true"));
        }
    }
    let mut details = vec![format!("{} stops and is removed", service.name())];
    details.push(if keep_data {
        format!("data in {data} is kept — reinstalling picks it up again")
    } else {
        format!("data in {data} is deleted permanently")
    });
    Ok(Plan {
        op: "service_uninstall",
        title: format!("uninstall {}", service.name()),
        risk: Risk::Destructive,
        details,
        confirm_word: Some(service.slug().to_string()),
        steps,
        reveal: vec![],
        secret_scope: None,
    })
}

/// Runs Laravel Reverb for one site under supervisor, as the app's owner.
pub fn reverb_for_site(site: &Site, port: u16) -> Result<Plan> {
    if site.framework != Framework::Laravel {
        bail!("Reverb runs inside a Laravel app");
    }
    let Some(app) = site.app_path.as_deref() else { bail!("the site has no app directory") };
    let slug: String = site
        .domains
        .first()
        .map(String::as_str)
        .unwrap_or("site")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let program = format!("sesame-{slug}-reverb");
    let script = format!(
        "cd {app_q}\n\
         grep -q '\"name\": \"laravel/reverb\"' composer.lock || {{ echo 'laravel/reverb is not installed in this app. Run: composer require laravel/reverb && php artisan reverb:install'; exit 1; }}\n\
         owner=$(stat -c %U artisan)\n\
         cat > /etc/supervisor/conf.d/{program}.conf <<CONF\n\
         [program:{program}]\n\
         command=php {app}/artisan reverb:start --host=127.0.0.1 --port={port}\n\
         directory={app}\n\
         user=$owner\n\
         autostart=true\nautorestart=true\nstopasgroup=true\nkillasgroup=true\nredirect_stderr=true\n\
         stdout_logfile={app}/storage/logs/reverb.log\nminfds=10000\n\
         CONF\n\
         supervisorctl reread >/dev/null\nsupervisorctl update\nsupervisorctl status {program}",
        app_q = q(app),
    );
    Ok(Plan {
        op: "reverb_enable",
        title: format!("run Reverb for {}", site.domains.first().cloned().unwrap_or_default()),
        risk: Risk::Change,
        details: vec![
            format!("supervisor program {program} as the app's owner on 127.0.0.1:{port}"),
            format!("route /app/* and /apps/* of the site to 127.0.0.1:{port}, and set REVERB_SERVER_PORT={port}"),
        ],
        confirm_word: None,
        steps: vec![Step::new("add the supervisor program", script)],
        reveal: vec![],
        secret_scope: None,
    })
}

#[cfg(test)]
mod tests {
    use sesame_core::snapshot::*;

    use super::*;

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    fn snapshot_with(dbs: &[DatabaseKind]) -> ServerSnapshot {
        ServerSnapshot {
            databases: dbs
                .iter()
                .map(|k| DatabaseEngine { kind: *k, version: Some("1".into()), listen: None, databases: vec![], sizes: vec![] })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn every_recipe_is_valid_shell_and_keeps_secrets_off_argv() {
        let empty = ServerSnapshot::default();
        for svc in Service::ALL {
            let plan = install(svc, &empty).unwrap();
            for step in &plan.steps {
                assert!(sh_ok(&step.script), "{svc:?}: {}", step.title);
            }
            for (_, secret) in plan.reveal.iter().filter(|(_, v)| v.len() == 32) {
                for step in &plan.steps {
                    for line in step.script.lines().filter(|l| l.contains(secret.as_str())) {
                        // Secrets live only in heredoc bodies or config lines, never in a command.
                        assert!(!line.contains("echo") && !line.contains("sed") && !line.starts_with("mysql"), "{line}");
                    }
                }
            }
            for keep in [true, false] {
                let plan = uninstall(svc, keep).unwrap();
                assert_eq!(plan.risk, Risk::Destructive);
                for step in &plan.steps {
                    assert!(sh_ok(&step.script));
                }
                assert_eq!(plan.steps.iter().any(|s| s.script.contains("rm -rf '/var/lib")), !keep, "{svc:?}");
            }
        }
    }

    #[test]
    fn detects_installed_and_conflicts() {
        let s = snapshot_with(&[DatabaseKind::MySql, DatabaseKind::Redis]);
        assert!(detect(Service::Redis, &s).installed);
        assert_eq!(detect(Service::MariaDb, &s).conflict.as_deref(), Some("MySQL"));
        assert!(install(Service::MariaDb, &s).is_err());
        assert!(install(Service::Redis, &s).is_err());
        assert!(install(Service::Postgres, &s).is_ok());
    }

    #[test]
    fn elasticsearch_password_is_captured_not_logged() {
        let plan = install(Service::Elasticsearch, &ServerSnapshot::default()).unwrap();
        let reset = plan.steps.iter().find(|s| s.script.contains("reset-password")).unwrap();
        assert_eq!(reset.capture_as.as_deref(), Some("ELASTIC_PASSWORD"));
        assert!(plan.steps.iter().any(|s| s.script.contains("install elasticsearch >/dev/null")));
        // ES 9 auto-config sets http.host: 0.0.0.0 — the recipe must override it.
        assert!(plan.steps.iter().any(|s| s.script.contains("http.host: 127.0.0.1")));
    }

    #[test]
    fn reverb_needs_a_laravel_site() {
        let site = Site {
            domains: vec!["shop.example.com".into()],
            app_path: Some("/var/www/shop".into()),
            framework: Framework::Laravel,
            ..Default::default()
        };
        let plan = reverb_for_site(&site, 8080).unwrap();
        assert!(sh_ok(&plan.steps[0].script));
        assert!(plan.steps[0].script.contains("[program:sesame-shop-example-com-reverb]"));
        assert!(reverb_for_site(&Site { framework: Framework::WordPress, ..site }, 8080).is_err());
    }
}

/// Database admin panels. They listen only on the server's loopback; the app
/// reaches them through an SSH tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    PhpMyAdmin,
    PgAdmin,
}

impl Panel {
    pub const ALL: [Panel; 2] = [Panel::PhpMyAdmin, Panel::PgAdmin];

    pub fn name(self) -> &'static str {
        match self {
            Panel::PhpMyAdmin => "phpMyAdmin",
            Panel::PgAdmin => "pgAdmin 4",
        }
    }

    /// Directory under /opt/sesame, also how discovery recognises it.
    pub fn dir(self) -> &'static str {
        match self {
            Panel::PhpMyAdmin => "phpmyadmin",
            Panel::PgAdmin => "pgadmin",
        }
    }

    pub fn port(self) -> u16 {
        match self {
            Panel::PhpMyAdmin => 8090,
            Panel::PgAdmin => 5050,
        }
    }

    pub fn needs(self) -> &'static [DatabaseKind] {
        match self {
            Panel::PhpMyAdmin => &[DatabaseKind::MySql, DatabaseKind::MariaDb],
            Panel::PgAdmin => &[DatabaseKind::Postgres],
        }
    }
}

pub fn panel_installed(panel: Panel, s: &ServerSnapshot) -> bool {
    s.tools.iter().any(|t| t == panel.dir())
}

fn web_site_step(name: &str, port: u16, root: &str, sock: &str, s: &ServerSnapshot) -> Step {
    use sesame_core::snapshot::WebServerKind;
    let web = s.web_servers.iter().map(|w| w.kind).find(|k| matches!(k, WebServerKind::Caddy | WebServerKind::FrankenPhp | WebServerKind::Nginx));
    let script = match web {
        Some(WebServerKind::Nginx) => format!(
            "cat > /etc/nginx/sites-available/sesame-{name}.conf <<'SITE'\nserver {{\n    listen 127.0.0.1:{port};\n    root {root};\n    index index.php;\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{sock};\n    }}\n}}\nSITE\n\
             ln -sf /etc/nginx/sites-available/sesame-{name}.conf /etc/nginx/sites-enabled/sesame-{name}.conf\n\
             if ! nginx -t >/tmp/sesame-web.log 2>&1; then tail -5 /tmp/sesame-web.log; rm -f /etc/nginx/sites-enabled/sesame-{name}.conf; exit 1; fi\nsystemctl reload nginx\necho 'listening on 127.0.0.1:{port}'"
        ),
        other => {
            let (dir, main, svc, ext) = if other == Some(WebServerKind::FrankenPhp) {
                ("/etc/frankenphp/Caddyfile.d", "/etc/frankenphp/Caddyfile", "frankenphp", "caddyfile")
            } else {
                ("/etc/caddy/sites", "/etc/caddy/Caddyfile", "caddy", "caddy")
            };
            let install = if other.is_none() { format!("{}\n", apt("caddy")) } else { String::new() };
            format!(
                "{install}install -d {dir}\n\
                 grep -q '^import {dir}/' {main} || printf '\\nimport {dir}/*.{ext}\\n' >> {main}\n\
                 cat > {dir}/sesame-{name}.{ext} <<'SITE'\nhttp://127.0.0.1:{port} {{\n\tbind 127.0.0.1\n\troot * {root}\n\tphp_fastcgi unix/{sock}\n\tfile_server\n}}\nSITE\n\
                 if ! {svc} validate --config {main} --adapter caddyfile >/tmp/sesame-web.log 2>&1; then tail -5 /tmp/sesame-web.log; rm -f {dir}/sesame-{name}.{ext}; exit 1; fi\n\
                 systemctl reload {svc}\necho 'listening on 127.0.0.1:{port}'"
            )
        }
    };
    Step::new("serve on 127.0.0.1 only", script)
}

pub fn install_panel(panel: Panel, s: &ServerSnapshot) -> Result<Plan> {
    if panel_installed(panel, s) {
        bail!("{} is already installed", panel.name());
    }
    if !panel.needs().iter().any(|k| s.databases.iter().any(|d| d.kind == *k)) {
        bail!("{} needs {} on this server first", panel.name(), if panel == Panel::PhpMyAdmin { "MySQL or MariaDB" } else { "PostgreSQL" });
    }
    let mut steps = vec![Step::new("check the system", preflight())];
    let mut reveal = Vec::new();
    let mut details = vec![format!("listens on the server's 127.0.0.1:{} only; Sesame opens it through an SSH tunnel", panel.port())];
    match panel {
        Panel::PhpMyAdmin => {
            let secret = generate_password();
            let pw = generate_password();
            let sock = "/run/php/sesame-pma.sock";
            steps.push(Step::new(
                "PHP and extensions",
                format!(
                    "v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;' 2>/dev/null || true)\n\
                     if [ -z \"$v\" ]; then {apt_default}; v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;'); fi\n\
                     {APT} install php$v-fpm php$v-mysql php$v-mbstring php$v-xml php$v-zip php$v-gd php$v-curl >/dev/null\n\
                     echo \"PHP $v\"",
                    apt_default = apt("php-fpm php-cli")
                ),
            ));
            steps.push(Step::new(
                "download phpMyAdmin (checksum verified)",
                format!(
                    "{}\n\
                     V=$(curl -fsSL https://www.phpmyadmin.net/home_page/version.txt | head -1)\n\
                     [ -n \"$V\" ] || {{ echo 'cannot find the phpMyAdmin version'; exit 1; }}\n\
                     tmp=$(mktemp -d); cd \"$tmp\"\n\
                     f=phpMyAdmin-$V-all-languages.tar.gz\n\
                     curl -fsSLO \"https://files.phpmyadmin.net/phpMyAdmin/$V/$f\"\n\
                     want=$(curl -fsSL \"https://files.phpmyadmin.net/phpMyAdmin/$V/$f.sha256\" | awk '{{print $1}}')\n\
                     got=$(sha256sum \"$f\" | awk '{{print $1}}')\n\
                     [ \"$want\" = \"$got\" ] || {{ echo 'checksum mismatch'; exit 1; }}\n\
                     tar xzf \"$f\"\n\
                     rm -rf /opt/sesame/phpmyadmin && install -d /opt/sesame\n\
                     mv \"phpMyAdmin-$V-all-languages\" /opt/sesame/phpmyadmin\n\
                     cd / && rm -rf \"$tmp\"\necho \"phpMyAdmin $V\"",
                    apt("curl ca-certificates")
                ),
            ));
            steps.push(Step::new(
                "user, config and PHP pool",
                format!(
                    "id sesame-pma >/dev/null 2>&1 || useradd --system --home /opt/sesame/phpmyadmin --shell /usr/sbin/nologin sesame-pma\n\
                     chown -R root:root /opt/sesame/phpmyadmin\n\
                     install -d -o sesame-pma -g sesame-pma -m 700 /opt/sesame/phpmyadmin/tmp\n\
                     umask 027\n\
                     cat > /opt/sesame/phpmyadmin/config.inc.php <<'PHP'\n<?php\n$cfg['blowfish_secret'] = '{secret}';\n$i = 1;\n$cfg['Servers'][$i]['auth_type'] = 'cookie';\n$cfg['Servers'][$i]['host'] = 'localhost';\n$cfg['Servers'][$i]['AllowNoPassword'] = false;\n$cfg['TempDir'] = '/opt/sesame/phpmyadmin/tmp';\nPHP\n\
                     chown root:sesame-pma /opt/sesame/phpmyadmin/config.inc.php && chmod 640 /opt/sesame/phpmyadmin/config.inc.php\n\
                     v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;')\n\
                     wg=www-data; getent group caddy >/dev/null && wg=caddy\n\
                     cat > /etc/php/$v/fpm/pool.d/sesame-pma.conf <<POOL\n[sesame-pma]\nuser = sesame-pma\ngroup = sesame-pma\nlisten = {sock}\nlisten.owner = sesame-pma\nlisten.group = $wg\nlisten.mode = 0660\npm = ondemand\npm.max_children = 4\nphp_admin_value[open_basedir] = /opt/sesame/phpmyadmin:/tmp\nPOOL\n\
                     systemctl reload php$v-fpm\necho 'pool ready'"
                ),
            ));
            steps.push(web_site_step("phpmyadmin", 8090, "/opt/sesame/phpmyadmin", sock, s));
            steps.push(Step::new(
                "MySQL admin login for phpMyAdmin",
                format!(
                    "mysql <<'SQL'\nCREATE USER IF NOT EXISTS 'sesame_pma'@'localhost' IDENTIFIED BY '{pw}';\nALTER USER 'sesame_pma'@'localhost' IDENTIFIED BY '{pw}';\nGRANT ALL PRIVILEGES ON *.* TO 'sesame_pma'@'localhost' WITH GRANT OPTION;\nFLUSH PRIVILEGES;\nSQL\necho 'login sesame_pma ready'"
                ),
            ));
            details.push("phpMyAdmin checked against its published SHA-256, its own system user and PHP pool".into());
            reveal.push(("PHPMYADMIN_USER".into(), "sesame_pma".into()));
            reveal.push(("PHPMYADMIN_PASSWORD".into(), pw));
        }
        Panel::PgAdmin => {
            let pw = generate_password();
            let db_pw = generate_password();
            steps.push(Step::new(
                "install pgAdmin 4 (Python)",
                format!(
                    "{}\n\
                     install -d /opt/sesame/pgadmin\n\
                     [ -x /opt/sesame/pgadmin/venv/bin/python ] || python3 -m venv /opt/sesame/pgadmin/venv\n\
                     /opt/sesame/pgadmin/venv/bin/pip install --quiet --upgrade pip\n\
                     /opt/sesame/pgadmin/venv/bin/pip install --quiet pgadmin4 gunicorn\n\
                     /opt/sesame/pgadmin/venv/bin/pip show pgadmin4 | grep '^Version'",
                    apt("python3-venv python3-pip libpq5 curl")
                ),
            ));
            steps.push(Step::new(
                "configure pgAdmin",
                format!(
                    "id sesame-pgadmin >/dev/null 2>&1 || useradd --system --home /var/lib/sesame-pgadmin --shell /usr/sbin/nologin sesame-pgadmin\n\
                     install -d -o sesame-pgadmin -g sesame-pgadmin -m 700 /var/lib/sesame-pgadmin /var/log/sesame-pgadmin\n\
                     PGA=$(/opt/sesame/pgadmin/venv/bin/python -c 'import pgadmin4; print(list(pgadmin4.__path__)[0])')\n\
                     cat > \"$PGA/config_local.py\" <<'PY'\nSERVER_MODE = True\nDATA_DIR = '/var/lib/sesame-pgadmin'\nLOG_FILE = '/var/log/sesame-pgadmin/pgadmin4.log'\nSQLITE_PATH = '/var/lib/sesame-pgadmin/pgadmin4.db'\nSESSION_DB_PATH = '/var/lib/sesame-pgadmin/sessions'\nSTORAGE_DIR = '/var/lib/sesame-pgadmin/storage'\nAZURE_CREDENTIAL_CACHE_DIR = '/var/lib/sesame-pgadmin/azure'\nKERBEROS_CCACHE_DIR = '/var/lib/sesame-pgadmin/kerberos'\nALLOW_SPECIAL_EMAIL_DOMAINS = ['local']\nPY\n\
                     cat > /var/lib/sesame-pgadmin/servers.json <<'JSON'\n{{\"Servers\": {{\"1\": {{\"Name\": \"This server\", \"Group\": \"Servers\", \"Host\": \"127.0.0.1\", \"Port\": 5432, \"MaintenanceDB\": \"postgres\", \"Username\": \"sesame_pgadmin\", \"SSLMode\": \"prefer\"}}}}}}\nJSON\n\
                     chown sesame-pgadmin /var/lib/sesame-pgadmin/servers.json\n\
                     runuser -u sesame-pgadmin -- sh -eu <<AS\nexport PGADMIN_SETUP_EMAIL='admin@sesame.local'\nexport PGADMIN_SETUP_PASSWORD='{pw}'\ncd \"$PGA\"\n/opt/sesame/pgadmin/venv/bin/python setup.py setup-db >/dev/null\n/opt/sesame/pgadmin/venv/bin/python setup.py load-servers /var/lib/sesame-pgadmin/servers.json --user admin@sesame.local >/dev/null\nAS\n\
                     cat > /etc/systemd/system/sesame-pgadmin.service <<UNIT\n[Unit]\nDescription=pgAdmin 4 (Sesame)\nAfter=network.target\n\n[Service]\nUser=sesame-pgadmin\nGroup=sesame-pgadmin\nWorkingDirectory=$PGA\nExecStart=/opt/sesame/pgadmin/venv/bin/gunicorn --bind 127.0.0.1:5050 --workers 1 --threads 25 pgAdmin4:app\nRestart=always\n\n[Install]\nWantedBy=multi-user.target\nUNIT\n\
                     systemctl daemon-reload\nsystemctl enable --now sesame-pgadmin\nsleep 3\nsystemctl is-active sesame-pgadmin"
                ),
            ));
            steps.push(Step::new(
                "PostgreSQL login for pgAdmin",
                format!(
                    "cd /tmp\nrunuser -u postgres -- psql -v ON_ERROR_STOP=1 -q <<'SQL'\nDO $$ BEGIN\n  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'sesame_pgadmin') THEN\n    CREATE ROLE sesame_pgadmin LOGIN SUPERUSER;\n  END IF;\nEND $$;\nALTER ROLE sesame_pgadmin PASSWORD '{db_pw}';\nSQL\necho 'login sesame_pgadmin ready'"
                ),
            ));
            details.push("pgAdmin 4 in its own Python environment under a system user, with this server's PostgreSQL pre-registered".into());
            reveal.push(("PGADMIN_EMAIL".into(), "admin@sesame.local".into()));
            reveal.push(("PGADMIN_PASSWORD".into(), pw));
            reveal.push(("PGADMIN_DB_USER".into(), "sesame_pgadmin".into()));
            reveal.push(("PGADMIN_DB_PASSWORD".into(), db_pw));
        }
    }
    Ok(Plan {
        op: "panel_install",
        title: format!("install {}", panel.name()),
        risk: Risk::Change,
        details,
        confirm_word: None,
        steps,
        reveal,
        secret_scope: None,
    })
}

pub fn uninstall_panel(panel: Panel) -> Result<Plan> {
    let script = match panel {
        Panel::PhpMyAdmin => "rm -f /etc/caddy/sites/sesame-phpmyadmin.caddy /etc/frankenphp/Caddyfile.d/sesame-phpmyadmin.caddyfile /etc/nginx/sites-enabled/sesame-phpmyadmin.conf /etc/nginx/sites-available/sesame-phpmyadmin.conf\n\
             for s in caddy frankenphp nginx; do systemctl is-active --quiet $s && systemctl reload $s || true; done\n\
             for f in /etc/php/*/fpm/pool.d/sesame-pma.conf; do [ -f \"$f\" ] && rm -f \"$f\" && systemctl reload php$(echo \"$f\" | cut -d/ -f4)-fpm; done\n\
             rm -rf /opt/sesame/phpmyadmin\nuserdel sesame-pma 2>/dev/null || true\n\
             mysql -e \"DROP USER IF EXISTS 'sesame_pma'@'localhost'\" 2>/dev/null || true\necho removed"
            .to_string(),
        Panel::PgAdmin => "systemctl disable --now sesame-pgadmin 2>/dev/null || true\nrm -f /etc/systemd/system/sesame-pgadmin.service\nsystemctl daemon-reload\n\
             rm -rf /opt/sesame/pgadmin /var/lib/sesame-pgadmin /var/log/sesame-pgadmin\nuserdel sesame-pgadmin 2>/dev/null || true\n\
             cd /tmp && runuser -u postgres -- psql -q -c 'DROP ROLE IF EXISTS sesame_pgadmin' 2>/dev/null || true\necho removed"
            .to_string(),
    };
    Ok(Plan {
        op: "panel_uninstall",
        title: format!("uninstall {}", panel.name()),
        risk: Risk::Destructive,
        details: vec![format!("{} and its admin login are removed; your databases are not touched", panel.name())],
        confirm_word: Some(panel.dir().to_string()),
        steps: vec![Step::new(format!("remove {}", panel.name()), script)],
        reveal: vec![],
        secret_scope: None,
    })
}

#[cfg(test)]
mod panel_tests {
    use sesame_core::snapshot::*;

    use super::*;

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    #[test]
    fn panels_are_valid_and_local_only() {
        let mut s = ServerSnapshot {
            web_servers: vec![WebServer { kind: WebServerKind::Caddy, version: None, config_path: None }],
            ..Default::default()
        };
        assert!(install_panel(Panel::PhpMyAdmin, &s).is_err(), "needs MySQL");
        s.databases = vec![
            DatabaseEngine { kind: DatabaseKind::MariaDb, version: None, listen: None, databases: vec![], sizes: vec![] },
            DatabaseEngine { kind: DatabaseKind::Postgres, version: None, listen: None, databases: vec![], sizes: vec![] },
        ];
        for p in Panel::ALL {
            let plan = install_panel(p, &s).unwrap();
            for st in &plan.steps {
                assert!(sh_ok(&st.script), "{p:?}: {}", st.title);
            }
            let all: String = plan.steps.iter().map(|x| x.script.as_str()).collect();
            assert!(all.contains("127.0.0.1"));
            assert!(!all.contains("0.0.0.0"));
            for st in &uninstall_panel(p).unwrap().steps {
                assert!(sh_ok(&st.script));
            }
        }
        let pma = install_panel(Panel::PhpMyAdmin, &s).unwrap();
        assert!(pma.steps.iter().any(|x| x.script.contains("checksum mismatch")));
        s.tools.push("phpmyadmin".into());
        assert!(panel_installed(Panel::PhpMyAdmin, &s));
    }
}
