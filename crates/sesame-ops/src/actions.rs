//! Built-in plans: service control, databases, maintenance mode.

use anyhow::{Result, bail};
use sesame_core::ops::Risk;
use sesame_core::snapshot::DatabaseKind;

use crate::{Plan, Step, generate_password};

/// Shell-quotes a value.
pub fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Names Sesame creates or acts on: letters, digits, `_`, `-`, `.`, `@`, `:`.
fn check_name(kind: &str, s: &str, allowed_extra: &str) -> Result<()> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || allowed_extra.contains(c));
    if !ok {
        bail!("invalid {kind} name “{s}”");
    }
    Ok(())
}

/// Credentials of a database administrator (MongoDB with auth on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admin {
    pub user: String,
    pub password: String,
}

/// Runs JavaScript through `mongosh --file`, which (unlike a heredoc on
/// stdin) exits non-zero on an uncaught error. The file is private and removed.
pub fn mongo_js(admin: Option<&Admin>, body: &str) -> String {
    let auth = admin
        .map(|a| format!("db.getSiblingDB('admin').auth('{}', '{}');\n", a.user, a.password))
        .unwrap_or_default();
    format!(
        "umask 077\njs=$(mktemp --suffix=.js)\ntrap 'rm -f \"$js\"' EXIT\ncat > \"$js\" <<'JS'\n{auth}{body}\nJS\nmongosh --quiet --file \"$js\""
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Systemd,
    Supervisor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    /// Enable at boot and start now.
    Enable,
    /// Stop now and do not start at boot.
    Disable,
}

pub fn service(kind: ServiceKind, name: &str, action: ServiceAction) -> Result<Plan> {
    check_name("service", name, "-.@:")?;
    let (verb, risk) = match action {
        ServiceAction::Start => ("start", Risk::Change),
        ServiceAction::Stop => ("stop", Risk::Change),
        ServiceAction::Restart => ("restart", Risk::Change),
        ServiceAction::Enable => ("enable", Risk::Change),
        ServiceAction::Disable => ("disable", Risk::Change),
    };
    let script = match (kind, action) {
        (ServiceKind::Systemd, ServiceAction::Enable) => format!("systemctl enable --now {}", q(name)),
        (ServiceKind::Systemd, ServiceAction::Disable) => format!("systemctl disable --now {}", q(name)),
        (ServiceKind::Systemd, _) => format!("systemctl {verb} {}", q(name)),
        (ServiceKind::Supervisor, ServiceAction::Enable | ServiceAction::Disable) => {
            bail!("supervisor programs are enabled by their config file")
        }
        (ServiceKind::Supervisor, _) => format!("supervisorctl {verb} {}", q(name)),
    };
    Ok(Plan {
        op: "service_action",
        title: format!("{verb} {name}"),
        risk,
        details: vec![format!("{verb} {name}")],
        confirm_word: None,
        steps: vec![Step::new(format!("{verb} {name}"), format!("{script}\necho done"))],
        reveal: vec![],
    })
}

/// Creates a database and a user that can only use that database.
pub fn db_create(engine: DatabaseKind, db: &str, user: &str, admin: Option<&Admin>) -> Result<Plan> {
    check_name("database", db, "")?;
    check_name("user", user, "")?;
    let pw = generate_password();
    let script = match engine {
        DatabaseKind::MySql | DatabaseKind::MariaDb => format!(
            "mysql <<'SQL'\n\
             CREATE DATABASE `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;\n\
             CREATE USER '{user}'@'localhost' IDENTIFIED BY '{pw}';\n\
             GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'localhost';\n\
             FLUSH PRIVILEGES;\n\
             SQL\necho created"
        ),
        DatabaseKind::Postgres => format!(
            "cd /tmp\nrunuser -u postgres -- psql -v ON_ERROR_STOP=1 -q <<'SQL'\n\
             CREATE ROLE \"{user}\" LOGIN PASSWORD '{pw}';\n\
             CREATE DATABASE \"{db}\" OWNER \"{user}\" ENCODING 'UTF8';\n\
             REVOKE ALL ON DATABASE \"{db}\" FROM PUBLIC;\n\
             SQL\necho created"
        ),
        DatabaseKind::MongoDb => format!(
            "{}\necho created",
            mongo_js(
                admin,
                &format!("db.getSiblingDB('{db}').createUser({{user: '{user}', pwd: '{pw}', roles: [{{role: 'readWrite', db: '{db}'}}]}});")
            )
        ),
        DatabaseKind::Redis => bail!("Redis databases are numbered, not created; use a Redis ACL user instead"),
    };
    Ok(Plan {
        op: "db_create",
        title: format!("create database {db}"),
        risk: Risk::Change,
        details: vec![
            format!("database {db}"),
            format!("user {user} with access to {db} only, from localhost"),
            "a 32-character password, shown once and kept in the encrypted vault".into(),
        ],
        confirm_word: None,
        steps: vec![Step::new(format!("create {db} and {user}"), script)],
        reveal: vec![("DB_DATABASE".into(), db.into()), ("DB_USERNAME".into(), user.into()), ("DB_PASSWORD".into(), pw)],
    })
}

/// Backs the database up to `/var/backups/sesame`, then drops it and its user.
pub fn db_drop(engine: DatabaseKind, db: &str, user: Option<&str>, admin: Option<&Admin>) -> Result<Plan> {
    check_name("database", db, "")?;
    if let Some(u) = user {
        check_name("user", u, "")?;
    }
    let backup_dir = "/var/backups/sesame";
    let (backup, drop) = match engine {
        DatabaseKind::MySql | DatabaseKind::MariaDb => (
            format!(
                "umask 077\nmkdir -p {backup_dir} && chmod 700 {backup_dir}\n\
                 f={backup_dir}/{db}-$(date +%Y%m%d-%H%M%S).sql.gz\n\
                 mysqldump --single-transaction --routines --triggers '{db}' | gzip > \"$f\"\necho \"backup: $f\""
            ),
            format!(
                "mysql <<'SQL'\nDROP DATABASE IF EXISTS `{db}`;\n{}SQL\necho dropped",
                user.map(|u| format!("DROP USER IF EXISTS '{u}'@'localhost';\n")).unwrap_or_default()
            ),
        ),
        DatabaseKind::Postgres => (
            format!(
                "umask 077\nmkdir -p {backup_dir} && chmod 700 {backup_dir}\ncd /tmp\n\
                 f={backup_dir}/{db}-$(date +%Y%m%d-%H%M%S).sql.gz\n\
                 runuser -u postgres -- pg_dump '{db}' | gzip > \"$f\"\necho \"backup: $f\""
            ),
            format!(
                "cd /tmp\nrunuser -u postgres -- psql -v ON_ERROR_STOP=1 -q <<'SQL'\nDROP DATABASE IF EXISTS \"{db}\";\n{}SQL\necho dropped",
                user.map(|u| format!("DROP ROLE IF EXISTS \"{u}\";\n")).unwrap_or_default()
            ),
        ),
        DatabaseKind::MongoDb => (
            {
                // The password goes in a private config file, not on the command line.
                let auth = admin
                    .map(|a| {
                        format!(
                            "cfg=$(mktemp)\ntrap 'rm -f \"$cfg\"' EXIT\nprintf 'password: %s\\n' '{}' > \"$cfg\"\n",
                            a.password
                        )
                    })
                    .unwrap_or_default();
                let flags = admin
                    .map(|a| format!(" --username='{}' --authenticationDatabase=admin --config=\"$cfg\"", a.user))
                    .unwrap_or_default();
                format!(
                    "umask 077\nmkdir -p {backup_dir} && chmod 700 {backup_dir}\n{auth}\
                     f={backup_dir}/{db}-$(date +%Y%m%d-%H%M%S).archive.gz\n\
                     mongodump --quiet --db='{db}' --archive=\"$f\" --gzip{flags}\necho \"backup: $f\""
                )
            },
            format!(
                "{}\necho dropped",
                mongo_js(
                    admin,
                    &format!(
                        "const d = db.getSiblingDB('{db}');\n{}d.dropDatabase();",
                        user.map(|u| format!("try {{ d.dropUser('{u}'); }} catch (e) {{}}\n")).unwrap_or_default()
                    )
                )
            ),
        ),
        DatabaseKind::Redis => bail!("Redis databases are numbered and cannot be dropped"),
    };
    Ok(Plan {
        op: "db_drop",
        title: format!("drop database {db}"),
        risk: Risk::Destructive,
        details: vec![
            format!("a compressed backup of {db} is written to {backup_dir} first"),
            format!("database {db} is deleted"),
        ]
        .into_iter()
        .chain(user.map(|u| format!("user {u} is deleted")))
        .collect(),
        confirm_word: Some(db.to_string()),
        steps: vec![
            Step::new(format!("back up {db}"), backup),
            Step::new(format!("drop {db}"), drop),
        ],
        reveal: vec![],
    })
}

/// `php artisan down` / `up`, run as the owner of the app.
pub fn maintenance(app_path: &str, down: bool) -> Result<Plan> {
    if !app_path.starts_with('/') || app_path.contains("..") {
        bail!("invalid app path “{app_path}”");
    }
    let cmd = if down { "down --retry=60" } else { "up" };
    let script = format!(
        "cd {path}\nowner=$(stat -c %U artisan)\nrunuser -u \"$owner\" -- php artisan {cmd}",
        path = q(app_path)
    );
    let what = if down { "maintenance mode on" } else { "maintenance mode off" };
    Ok(Plan {
        op: "site_maintenance",
        title: format!("{what}: {app_path}"),
        risk: Risk::Change,
        details: vec![format!("php artisan {cmd} in {app_path}, as the app's owner")],
        confirm_word: None,
        steps: vec![Step::new(what, script)],
        reveal: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    #[test]
    fn service_plans() {
        let p = service(ServiceKind::Systemd, "php8.5-fpm", ServiceAction::Disable).unwrap();
        assert!(p.steps[0].script.contains("systemctl disable --now 'php8.5-fpm'"));
        let p = service(ServiceKind::Supervisor, "shop-queue:shop-queue_00", ServiceAction::Restart).unwrap();
        assert!(p.steps[0].script.contains("supervisorctl restart 'shop-queue:shop-queue_00'"));
        assert!(service(ServiceKind::Systemd, "x; rm -rf /", ServiceAction::Stop).is_err());
        assert!(service(ServiceKind::Supervisor, "x", ServiceAction::Enable).is_err());
    }

    #[test]
    fn db_create_keeps_password_out_of_argv() {
        for engine in [DatabaseKind::MySql, DatabaseKind::Postgres, DatabaseKind::MongoDb] {
            let p = db_create(engine, "shop", "shop", None).unwrap();
            let pw = &p.reveal.iter().find(|(k, _)| k == "DB_PASSWORD").unwrap().1;
            let script = &p.steps[0].script;
            assert!(sh_ok(script), "{script}");
            // The password only appears inside the heredoc body, never on a command line.
            let heredoc_body: String = script
                .split("<<'")
                .nth(1)
                .unwrap()
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(heredoc_body.contains(pw.as_str()));
            assert_eq!(script.matches(pw.as_str()).count(), 1);
        }
        assert!(db_create(DatabaseKind::MySql, "shop`; DROP", "x", None).is_err());
        assert!(db_create(DatabaseKind::Redis, "a", "b", None).is_err());
    }

    #[test]
    fn db_drop_backs_up_first_and_needs_the_name() {
        let p = db_drop(DatabaseKind::MySql, "shop", Some("shop"), None).unwrap();
        assert_eq!(p.risk, Risk::Destructive);
        assert_eq!(p.confirm_word.as_deref(), Some("shop"));
        assert!(p.steps[0].script.contains("mysqldump"));
        assert!(p.steps[1].script.contains("DROP DATABASE IF EXISTS `shop`"));
        for s in &p.steps {
            assert!(sh_ok(&s.script));
        }
    }

    #[test]
    fn mongo_runs_from_a_file_and_authenticates() {
        let admin = Admin { user: "sesame_admin".into(), password: "P4ssw0rdP4ssw0rdP4ssw0rdP4ssw0rd".into() };
        let create = db_create(DatabaseKind::MongoDb, "shop", "shop", Some(&admin)).unwrap();
        let script = &create.steps[0].script;
        assert!(script.contains("mongosh --quiet --file"));
        assert!(script.contains("auth('sesame_admin'"));
        assert!(sh_ok(script));
        let drop = db_drop(DatabaseKind::MongoDb, "shop", Some("shop"), Some(&admin)).unwrap();
        let backup = &drop.steps[0].script;
        assert!(sh_ok(backup));
        assert!(backup.contains("--config=\"$cfg\""));
        // The password never appears on the mongodump command line.
        let dump_line = backup.lines().find(|l| l.starts_with("mongodump")).unwrap();
        assert!(!dump_line.contains(&admin.password));
    }

    #[test]
    fn maintenance_runs_as_owner() {
        let p = maintenance("/var/www/shop", true).unwrap();
        assert!(p.steps[0].script.contains("runuser -u \"$owner\" -- php artisan down"));
        assert!(sh_ok(&p.steps[0].script));
        assert!(maintenance("relative/path", true).is_err());
    }
}

/// GitHub's published SSH host key (ed25519). Pinned so a deploy never
/// trusts an impostor `github.com`.
pub const GITHUB_HOST_KEY: &str =
    "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

fn slug_of(app_path: &str) -> String {
    app_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("site")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

fn check_path(app_path: &str) -> Result<()> {
    if !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') || app_path.len() < 2 {
        bail!("invalid app path “{app_path}”");
    }
    Ok(())
}

/// Runs `body` as the owner of `app_path` (or `fallback_owner` when the
/// directory does not exist yet), via a heredoc so nothing needs quoting.
fn as_owner(app_path: &str, body: &str) -> String {
    format!(
        "owner=$(stat -c %U {p} 2>/dev/null || stat -c %U \"$(dirname {p})\")\n\
         home=$(getent passwd \"$owner\" | cut -d: -f6)\n\
         runuser -u \"$owner\" -- env HOME=\"$home\" sh -eu <<'AS_OWNER'\n{body}\nAS_OWNER",
        p = q(app_path)
    )
}

/// Creates a deploy key for the site (owned by the app's user, never leaving
/// the server) and prints its public half for GitHub.
pub fn git_connect(app_path: &str) -> Result<Plan> {
    check_path(app_path)?;
    let slug = slug_of(app_path);
    let key = format!("$HOME/.ssh/sesame_{slug}");
    let body = format!(
        "mkdir -p \"$HOME/.ssh\" && chmod 700 \"$HOME/.ssh\"\n\
         [ -f \"{key}\" ] || ssh-keygen -q -t ed25519 -N '' -C 'sesame@{slug}' -f \"{key}\"\n\
         touch \"$HOME/.ssh/known_hosts\"\n\
         grep -q '^github.com ssh-ed25519 ' \"$HOME/.ssh/known_hosts\" || echo '{GITHUB_HOST_KEY}' >> \"$HOME/.ssh/known_hosts\"\n\
         cat \"{key}.pub\""
    );
    Ok(Plan {
        op: "git_connect",
        title: format!("create a deploy key for {app_path}"),
        risk: Risk::Change,
        details: vec![
            format!("an ed25519 key in the app owner's ~/.ssh/sesame_{slug}; the private key never leaves the server"),
            "GitHub's host key is pinned in known_hosts".into(),
            "the public key is added to the repository as a read-only deploy key".into(),
        ],
        confirm_word: None,
        steps: vec![Step::new("create the deploy key", as_owner(app_path, &body)).capture("DEPLOY_PUBLIC_KEY")],
        reveal: vec![],
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployOptions {
    pub laravel: bool,
    pub migrate: bool,
    /// Supervisor programs to restart after the code changes (Octane, Horizon…).
    pub restart: Vec<String>,
}

/// Fetches `branch` and checks it out in `app_path`, then runs the Laravel
/// steps. Untracked files (`.env`, `storage/`) are kept.
pub fn deploy(app_path: &str, repo: &str, branch: &str, opts: &DeployOptions) -> Result<Plan> {
    check_path(app_path)?;
    let repo_ok = repo.split('/').count() == 2
        && repo.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    let branch_ok = !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.contains("..")
        && branch.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if !repo_ok || !branch_ok {
        bail!("invalid repository or branch");
    }
    for p in &opts.restart {
        check_name("program", p, "-.:")?;
    }
    let slug = slug_of(app_path);
    let ssh = format!("ssh -i $HOME/.ssh/sesame_{slug} -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes");
    let url = format!("git@github.com:{repo}.git");
    let fetch = format!(
        "export GIT_SSH_COMMAND='{ssh}'\n\
         cd {p}\n\
         if [ ! -d .git ]; then\n\
         \x20 git init -q\n\
         \x20 git remote add origin '{url}'\n\
         fi\n\
         git remote set-url origin '{url}'\n\
         git fetch --quiet origin '{branch}'\n\
         git checkout -q -B '{branch}' 'origin/{branch}'\n\
         git reset -q --hard 'origin/{branch}'\n\
         git log -1 --format='%h %s'",
        p = q(app_path)
    );
    let mut steps = vec![Step::new(format!("fetch {repo}@{branch}"), as_owner(app_path, &fetch))];
    if opts.laravel {
        let mut body = format!(
            "cd {p}\n\
             if [ -f composer.json ]; then composer install --no-dev --optimize-autoloader --no-interaction --no-progress 2>&1 | tail -3; fi\n",
            p = q(app_path)
        );
        if opts.migrate {
            body.push_str("php artisan migrate --force\n");
        }
        body.push_str("php artisan optimize\nphp artisan queue:restart || true");
        steps.push(Step::new("composer, migrations, caches", as_owner(app_path, &body)));
    }
    if !opts.restart.is_empty() {
        let names: Vec<String> = opts.restart.iter().map(|p| q(p)).collect();
        steps.push(Step::new("restart workers", format!("supervisorctl restart {}", names.join(" "))));
    }
    Ok(Plan {
        op: "deploy",
        title: format!("deploy {repo}@{branch} to {app_path}"),
        risk: Risk::Change,
        details: vec![
            format!("{app_path} is reset to the latest commit of {branch}; .env and other untracked files stay"),
            if opts.laravel { "composer install, migrations, optimize, queue restart".into() } else { "no build steps".into() },
        ],
        confirm_word: None,
        steps,
        reveal: vec![],
    })
}

#[cfg(test)]
mod deploy_tests {
    use super::*;

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    #[test]
    fn connect_and_deploy_scripts() {
        let c = git_connect("/var/www/shop").unwrap();
        assert_eq!(c.steps[0].capture_as.as_deref(), Some("DEPLOY_PUBLIC_KEY"));
        assert!(sh_ok(&c.steps[0].script));
        assert!(c.steps[0].script.contains("runuser -u \"$owner\""));

        let opts = DeployOptions { laravel: true, migrate: true, restart: vec!["shop-octane".into()] };
        let d = deploy("/var/www/shop", "rasul-chopurov/shop", "main", &opts).unwrap();
        for s in &d.steps {
            assert!(sh_ok(&s.script), "{}", s.title);
        }
        assert!(d.steps[0].script.contains("StrictHostKeyChecking=yes"));
        assert!(d.steps[1].script.contains("migrate --force"));
        assert!(d.steps[2].script.contains("supervisorctl restart 'shop-octane'"));
        assert!(deploy("/var/www/shop", "a/b", "--upload-pack=x", &opts).is_err());
        assert!(deploy("/var/www/shop", "a;b/c", "main", &opts).is_err());
    }
}

/// Installs a certificate and its private key under `/etc/ssl/sesame/<name>/`
/// after checking they belong together. The key travels in the script on
/// stdin and is readable only by root and the web server group.
pub fn cert_install(name: &str, cert_pem: &str, key_pem: &str) -> Result<Plan> {
    check_name("certificate", name, "-.")?;
    let cert = cert_pem.trim();
    let key = key_pem.trim();
    if !cert.starts_with("-----BEGIN CERTIFICATE-----") || !cert.ends_with("-----END CERTIFICATE-----") {
        bail!("the certificate must be PEM (-----BEGIN CERTIFICATE-----)");
    }
    let key_ok = key.starts_with("-----BEGIN") && key.contains("PRIVATE KEY-----") && key.ends_with("PRIVATE KEY-----");
    if !key_ok {
        bail!("the private key must be PEM (-----BEGIN PRIVATE KEY-----)");
    }
    if cert.contains("SESAME_PEM") || key.contains("SESAME_PEM") {
        bail!("unexpected content in PEM");
    }
    let dir = format!("/etc/ssl/sesame/{name}");
    let script = format!(
        "umask 077\n\
         tmp=$(mktemp -d)\n\
         trap 'rm -rf \"$tmp\"' EXIT\n\
         cat > \"$tmp/cert.pem\" <<'SESAME_PEM'\n{cert}\nSESAME_PEM\n\
         cat > \"$tmp/key.pem\" <<'SESAME_PEM'\n{key}\nSESAME_PEM\n\
         a=$(openssl x509 -noout -pubkey -in \"$tmp/cert.pem\" | sha256sum)\n\
         b=$(openssl pkey -pubout -in \"$tmp/key.pem\" | sha256sum)\n\
         [ \"$a\" = \"$b\" ] || {{ echo 'the private key does not match the certificate'; exit 1; }}\n\
         grp=root\n\
         for g in caddy www-data nginx; do getent group \"$g\" >/dev/null && {{ grp=$g; break; }}; done\n\
         install -d -m 750 -o root -g \"$grp\" /etc/ssl/sesame {dir}\n\
         install -m 644 -o root -g \"$grp\" \"$tmp/cert.pem\" {dir}/cert.pem\n\
         install -m 640 -o root -g \"$grp\" \"$tmp/key.pem\" {dir}/key.pem\n\
         openssl x509 -noout -subject -enddate -ext subjectAltName -in {dir}/cert.pem"
    );
    Ok(Plan {
        op: "cert_install",
        title: format!("install certificate {name}"),
        risk: Risk::Change,
        details: vec![
            format!("{dir}/cert.pem and key.pem (key readable by root and the web server group only)"),
            "the key is checked against the certificate first".into(),
        ],
        confirm_word: None,
        steps: vec![Step::new(format!("install {name}"), script)],
        reveal: vec![],
    })
}

#[cfg(test)]
mod cert_tests {
    use super::*;

    #[test]
    fn cert_install_validates_pem_and_shell() {
        let cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        let key = "-----BEGIN PRIVATE KEY-----\nMIIE\n-----END PRIVATE KEY-----";
        let p = cert_install("response.az", cert, key).unwrap();
        let ok = std::process::Command::new("sh").args(["-n", "-c", &p.steps[0].script]).status().unwrap().success();
        assert!(ok);
        assert!(p.steps[0].script.contains("install -m 640"));
        assert!(cert_install("response.az", "not a cert", key).is_err());
        assert!(cert_install("../etc", cert, key).is_err());
    }
}
