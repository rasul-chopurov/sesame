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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Redis,
    Postgres,
    MariaDb,
    MongoDb,
    Elasticsearch,
    RustFs,
}

impl Service {
    pub const ALL: [Service; 6] = [
        Service::Redis,
        Service::Postgres,
        Service::MariaDb,
        Service::MongoDb,
        Service::Elasticsearch,
        Service::RustFs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Service::Redis => "Redis",
            Service::Postgres => "PostgreSQL",
            Service::MariaDb => "MariaDB",
            Service::MongoDb => "MongoDB",
            Service::Elasticsearch => "Elasticsearch",
            Service::RustFs => "RustFS",
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
    };
    let conflict = match service {
        Service::MariaDb if found.is_none() && engine(s, DatabaseKind::MySql).is_some() => Some("MySQL".to_string()),
        Service::RustFs if found.is_none() && s.ports.iter().any(|p| p.port == 9000) => Some("port 9000".to_string()),
        _ => None,
    };
    Status { installed: found.is_some(), version: found.filter(|v| !v.is_empty()), conflict }
}

const APT: &str = "apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q";

fn preflight() -> String {
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
    };
    let mut steps = vec![Step::new(
        format!("stop {unit}"),
        format!("systemctl disable --now {unit} 2>/dev/null || true\necho stopped"),
    )];
    if service == Service::RustFs {
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
                .map(|k| DatabaseEngine { kind: *k, version: Some("1".into()), listen: None, databases: vec![] })
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
