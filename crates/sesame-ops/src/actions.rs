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
            "a 32-character password, shown once and kept in the Keychain".into(),
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
