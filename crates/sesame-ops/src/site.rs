//! The "new site" wizard: an isolated site with its own Linux user, its own
//! PHP-FPM pool (or Octane), its own web-server file, database and cron.

use anyhow::{Result, bail};
use sesame_core::ops::Risk;
use sesame_core::snapshot::{DatabaseKind, ServerSnapshot, WebServerKind};

use crate::actions::q;
use crate::{Plan, Step, generate_password};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// Fresh `laravel/laravel` skeleton.
    Laravel,
    /// Plain files served as they are.
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// A dedicated PHP-FPM pool running as the site user.
    Fpm,
    /// Laravel Octane (FrankenPHP) under supervisor as the site user.
    Octane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub repo: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSite {
    pub domain: String,
    pub aliases: Vec<String>,
    pub kind: SiteKind,
    pub runtime: Runtime,
    /// Deploy from a public GitHub repository instead of a skeleton.
    pub source: Option<Source>,
    /// Directory inside the app served as the web root (`public`, `website`, `.`).
    pub web_root: String,
    pub database: Option<DatabaseKind>,
    pub scheduler: bool,
}

/// `shop.example.com` → `shop_example_com`, ≤ 28 chars (Linux user names ≤ 32 with the `s_` prefix).
pub fn slug(domain: &str) -> String {
    let s: String = domain
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    s.trim_matches('_').chars().take(28).collect()
}

fn valid_domain(d: &str) -> bool {
    d.len() <= 253
        && d.contains('.')
        && d.split('.').all(|l| {
            !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

struct Ctx {
    web: WebServerKind,
    php: Option<String>,
    web_group: &'static str,
    /// The user the web server reads files as (None when it runs as root).
    web_user: Option<&'static str>,
    cert: Option<String>,
}

fn context(snap: &ServerSnapshot, domain: &str) -> Ctx {
    let web = snap
        .web_servers
        .iter()
        .map(|w| w.kind)
        .find(|k| matches!(k, WebServerKind::Caddy | WebServerKind::FrankenPhp | WebServerKind::Nginx))
        .unwrap_or(WebServerKind::Caddy);
    let php = snap
        .php
        .iter()
        .find(|p| p.fpm_running || !p.fpm_pools.is_empty())
        .or(snap.php.first())
        .map(|p| p.version.split('.').take(2).collect::<Vec<_>>().join("."));
    let web_group = match web {
        WebServerKind::Caddy => "caddy",
        WebServerKind::FrankenPhp => "root",
        _ => "www-data",
    };
    let web_user = match web {
        WebServerKind::Caddy => Some("caddy"),
        WebServerKind::Nginx | WebServerKind::Apache => Some("www-data"),
        WebServerKind::FrankenPhp => None,
    };
    // An installed certificate that covers the domain (e.g. a Cloudflare Origin wildcard).
    let covers = |name: &str| name == domain || name.strip_prefix("*.").is_some_and(|z| domain.ends_with(&format!(".{z}")) && domain.matches('.').count() == z.matches('.').count() + 1);
    let cert = snap
        .certificates
        .iter()
        .find(|c| c.names.iter().any(|n| covers(n)))
        .map(|c| c.path.clone());
    Ctx { web, php, web_group, web_user, cert }
}

pub fn new_site(site: &NewSite, snap: &ServerSnapshot) -> Result<Plan> {
    let domain = site.domain.trim().to_ascii_lowercase();
    if !valid_domain(&domain) || site.aliases.iter().any(|a| !valid_domain(a)) {
        bail!("invalid domain");
    }
    if snap.sites.iter().any(|s| s.domains.contains(&domain)) {
        bail!("{domain} is already served on this server");
    }
    if site.web_root.contains("..") || site.web_root.starts_with('/') || !site.web_root.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c)) {
        bail!("invalid web root");
    }
    if let Some(src) = &site.source {
        let ok = src.repo.split('/').count() == 2
            && src.repo.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
            && !src.branch.starts_with('-')
            && !src.branch.contains("..")
            && src.branch.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
        if !ok {
            bail!("invalid repository or branch");
        }
    }
    let ctx = context(snap, &domain);
    let php_needed = site.kind == SiteKind::Laravel;
    let php = match (&ctx.php, php_needed) {
        (Some(v), _) => v.clone(),
        (None, true) => bail!("PHP is not installed on this server"),
        (None, false) => String::new(),
    };
    let slug = slug(&domain);
    let user = format!("s_{slug}");
    let home = format!("/home/{user}");
    let app = format!("{home}/{domain}");
    let root = if site.web_root == "." || site.web_root.is_empty() { app.clone() } else { format!("{app}/{}", site.web_root.trim_end_matches('/')) };
    let sock = format!("/run/php/{user}.sock");
    let mut steps = Vec::new();
    let mut details = Vec::new();
    let mut reveal = Vec::new();

    // 1. User and home.
    steps.push(Step::new(
        format!("create user {user}"),
        format!(
            "if id {user} >/dev/null 2>&1; then echo 'user {user} already exists'; exit 1; fi\n\
             if getent group {user} >/dev/null; then useradd --create-home --home-dir {home} --shell /bin/bash -g {user} {user}; else useradd --create-home --home-dir {home} --shell /bin/bash {user}; fi\n\
             chmod 750 {home}\n\
             install -d -o {user} -g {user} -m 750 {app}\n\
             command -v setfacl >/dev/null || apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q install acl >/dev/null\n\
             echo {home}"
        ),
    ));
    details.push(format!("Linux user {user}; {app} readable only by it and the web server"));

    // 2. Code.
    let as_user = |body: &str| format!("runuser -u {user} -- env HOME={home} sh -eu <<'AS_USER'\ncd {app}\n{body}\nAS_USER");
    match (&site.source, site.kind) {
        (Some(src), _) => {
            steps.push(Step::new(
                format!("clone {}@{}", src.repo, src.branch),
                as_user(&format!(
                    "git clone --quiet --branch {b} https://github.com/{r}.git .\ngit log -1 --format='%h %s'",
                    b = q(&src.branch),
                    r = src.repo
                )),
            ));
            details.push(format!("code from github.com/{} ({})", src.repo, src.branch));
        }
        (None, SiteKind::Laravel) => {
            steps.push(Step::new(
                "create a Laravel app",
                as_user("composer create-project --no-interaction --prefer-dist --quiet laravel/laravel .\nphp artisan --version"),
            ));
            details.push("a fresh Laravel app".into());
        }
        (None, SiteKind::Static) => {
            steps.push(Step::new(
                "add a placeholder page",
                as_user(&format!(
                    "mkdir -p {r}\nprintf '<!doctype html><title>{domain}</title><h1>{domain}</h1><p>Opened with Sesame.</p>\\n' > {r}/index.html",
                    r = q(&root)
                )),
            ));
            details.push("a placeholder page".into());
        }
    }

    // 3. Laravel setup (composer for repos, storage permissions, scheduler).
    if site.kind == SiteKind::Laravel {
        let mut body = String::from(
            "if [ -f composer.json ] && [ ! -d vendor ]; then composer install --no-dev --optimize-autoloader --no-interaction --quiet; fi\n\
             if [ ! -f .env ] && [ -f .env.example ]; then cp .env.example .env; fi\n\
             grep -q '^APP_KEY=base64' .env || php artisan key:generate --force\n",
        );
        body.push_str(&format!("sed -i 's#^APP_URL=.*#APP_URL=https://{domain}#' .env\n"));
        body.push_str("chmod 640 .env\nchmod -R g+rwX storage bootstrap/cache");
        steps.push(Step::new("configure Laravel", as_user(&body)));
    }

    // 4. Database.
    if let Some(engine) = site.database {
        let db = slug.clone();
        let pw = generate_password();
        let create = match engine {
            DatabaseKind::MySql | DatabaseKind::MariaDb => format!(
                "mysql <<'SQL'\nCREATE DATABASE `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;\n\
                 CREATE USER '{db}'@'localhost' IDENTIFIED BY '{pw}';\nGRANT ALL PRIVILEGES ON `{db}`.* TO '{db}'@'localhost';\nFLUSH PRIVILEGES;\nSQL"
            ),
            DatabaseKind::Postgres => format!(
                "cd /tmp\nrunuser -u postgres -- psql -v ON_ERROR_STOP=1 -q <<'SQL'\nCREATE ROLE \"{db}\" LOGIN PASSWORD '{pw}';\n\
                 CREATE DATABASE \"{db}\" OWNER \"{db}\" ENCODING 'UTF8';\nREVOKE ALL ON DATABASE \"{db}\" FROM PUBLIC;\nSQL"
            ),
            _ => bail!("sites get MySQL, MariaDB or PostgreSQL databases"),
        };
        let (conn, port) = if engine == DatabaseKind::Postgres { ("pgsql", "5432") } else { ("mysql", "3306") };
        // PHP needs the driver for this database.
        let ext = if site.kind == SiteKind::Laravel {
            format!(
                "if ! php{php} -m | grep -qi '^pdo_{conn}$'; then\n\
                 \x20 apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q install php{php}-{conn} >/dev/null\n\
                 \x20 systemctl reload php{php}-fpm 2>/dev/null || true\n\
                 \x20 echo 'installed php{php}-{conn}'\n\
                 fi\n"
            )
        } else {
            String::new()
        };
        let env = if site.kind == SiteKind::Laravel {
            format!(
                "\n{}",
                as_user(&format!(
                    "set_env() {{ if grep -q \"^$1=\" .env || grep -q \"^# $1=\" .env; then sed -i -E \"s|^#? ?$1=.*|$1=$2|\" .env; else echo \"$1=$2\" >> .env; fi; }}\n\
                     set_env DB_CONNECTION {conn}\nset_env DB_HOST 127.0.0.1\nset_env DB_PORT {port}\nset_env DB_DATABASE {db}\nset_env DB_USERNAME {db}\nset_env DB_PASSWORD {pw}\n\
                     php artisan migrate --force --no-interaction"
                ))
            )
        } else {
            String::new()
        };
        steps.push(Step::new(format!("create database {db}"), format!("{ext}{create}{env}")));
        details.push(format!("database {db} with its own user"));
        reveal.push(("DB_DATABASE".into(), db.clone()));
        reveal.push(("DB_USERNAME".into(), db.clone()));
        reveal.push(("DB_PASSWORD".into(), pw));
    }

    // 5. PHP runtime.
    let mut upstream = None;
    if site.kind == SiteKind::Laravel {
        match site.runtime {
            Runtime::Fpm => {
                steps.push(Step::new(
                    format!("PHP {php} pool for {user}"),
                    format!(
                        "test -d /etc/php/{php}/fpm/pool.d || {{ echo 'php{php}-fpm is not installed'; exit 1; }}\n\
                         cat > /etc/php/{php}/fpm/pool.d/{user}.conf <<'POOL'\n\
                         [{user}]\nuser = {user}\ngroup = {user}\nlisten = {sock}\nlisten.owner = {user}\nlisten.group = {wg}\nlisten.mode = 0660\n\
                         pm = ondemand\npm.max_children = 8\npm.process_idle_timeout = 30s\n\
                         php_admin_value[open_basedir] = {app}:/tmp\nphp_admin_value[upload_tmp_dir] = /tmp\n\
                         POOL\n\
                         if ! php-fpm{php} -t >/tmp/sesame-fpm.log 2>&1; then tail -5 /tmp/sesame-fpm.log; rm -f /etc/php/{php}/fpm/pool.d/{user}.conf; exit 1; fi\n\
                         systemctl reload php{php}-fpm\necho 'pool ready'",
                        wg = ctx.web_group
                    ),
                ));
                details.push(format!("its own PHP {php} FPM pool as {user}, open_basedir {app}"));
            }
            Runtime::Octane => {
                let port = (8100u16..8400).find(|p| !snap.ports.iter().any(|x| x.port == *p)).unwrap_or(8100);
                let program = format!("sesame-{slug}-octane");
                steps.push(Step::new(
                    "Octane (FrankenPHP)",
                    format!(
                        "{}\ncat > /etc/supervisor/conf.d/{program}.conf <<CONF\n[program:{program}]\n\
                         command=php {app}/artisan octane:start --server=frankenphp --host=127.0.0.1 --port={port}\n\
                         directory={app}\nuser={user}\nenvironment=HOME=\"{home}\"\nautostart=true\nautorestart=true\nstopasgroup=true\nkillasgroup=true\n\
                         redirect_stderr=true\nstdout_logfile={app}/storage/logs/octane.log\nCONF\nsupervisorctl reread >/dev/null\nsupervisorctl update",
                        as_user("grep -q '\"name\": \"laravel/octane\"' composer.lock || composer require --no-interaction --quiet laravel/octane\nphp artisan octane:install --server=frankenphp --no-interaction")
                    ),
                ));
                upstream = Some(format!("127.0.0.1:{port}"));
                details.push(format!("Octane as {user} on 127.0.0.1:{port} under supervisor"));
            }
        }
    }

    // 6. Web server file.
    let hosts: Vec<String> = std::iter::once(domain.clone()).chain(site.aliases.iter().cloned()).collect();
    let tls = match &ctx.cert {
        Some(cert) => {
            let key = cert.replace("cert.pem", "key.pem").replace("fullchain.pem", "privkey.pem");
            format!("\ttls {cert} {key}\n")
        }
        None => String::new(),
    };
    let handler = match (site.kind, &upstream) {
        (SiteKind::Laravel, Some(up)) => format!("\t@static file\n\thandle @static {{\n\t\tfile_server\n\t}}\n\treverse_proxy {up}\n"),
        (SiteKind::Laravel, None) => format!("\tphp_fastcgi unix/{sock}\n\tfile_server\n"),
        (SiteKind::Static, _) => "\tfile_server\n".into(),
    };
    let web_step = match ctx.web {
        WebServerKind::Nginx => {
            let php_loc = if site.kind == SiteKind::Laravel && upstream.is_none() {
                format!("    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{sock};\n    }}\n")
            } else if let Some(up) = &upstream {
                format!("    location / {{ proxy_pass http://{up}; proxy_set_header Host $host; proxy_set_header X-Forwarded-Proto $scheme; }}\n")
            } else {
                String::new()
            };
            format!(
                "cat > /etc/nginx/sites-available/sesame-{slug}.conf <<'SITE'\nserver {{\n    listen 80;\n    server_name {names};\n    root {root};\n    index index.php index.html;\n    location / {{ try_files $uri $uri/ /index.php?$query_string; }}\n{php_loc}}}\nSITE\n\
                 ln -sf /etc/nginx/sites-available/sesame-{slug}.conf /etc/nginx/sites-enabled/sesame-{slug}.conf\n\
                 if ! nginx -t >/tmp/sesame-web.log 2>&1; then tail -5 /tmp/sesame-web.log; rm -f /etc/nginx/sites-enabled/sesame-{slug}.conf; exit 1; fi\n\
                 systemctl reload nginx\necho 'site enabled'",
                names = hosts.join(" ")
            )
        }
        WebServerKind::FrankenPhp | WebServerKind::Caddy => {
            let (dir, main, svc) = if ctx.web == WebServerKind::FrankenPhp {
                ("/etc/frankenphp/Caddyfile.d", "/etc/frankenphp/Caddyfile", "frankenphp")
            } else {
                ("/etc/caddy/sites", "/etc/caddy/Caddyfile", "caddy")
            };
            let ext = if ctx.web == WebServerKind::FrankenPhp { "caddyfile" } else { "caddy" };
            format!(
                "install -d {dir}\n\
                 grep -q '^import {dir}/' {main} || printf '\\nimport {dir}/*.{ext}\\n' >> {main}\n\
                 cat > {dir}/sesame-{slug}.{ext} <<'SITE'\n{names} {{\n{tls}\troot * {root}\n\tencode zstd gzip\n{handler}}}\nSITE\n\
                 if ! {svc} validate --config {main} --adapter caddyfile >/tmp/sesame-web.log 2>&1; then tail -5 /tmp/sesame-web.log; rm -f {dir}/sesame-{slug}.{ext}; exit 1; fi\n\
                 systemctl reload {svc}\necho 'site enabled'",
                names = hosts.join(", ")
            )
        }
        WebServerKind::Apache => bail!("Apache sites are not supported by the wizard yet"),
    };
    // The web server may only pass through the home and app directories and
    // read the web root — not .env or the code. ACLs apply without a restart.
    if let Some(w) = ctx.web_user {
        steps.push(Step::new(
            format!("let {w} read {root}"),
            format!(
                "install -d -o {user} -g {user} {root_q}\n\
                 setfacl -m u:{w}:x {home} {app_q}\n\
                 d={root_q}\nwhile [ \"$d\" != {app_q} ] && [ \"$d\" != / ]; do setfacl -m u:{w}:x \"$d\"; d=$(dirname \"$d\"); done\n\
                 setfacl -R -m u:{w}:rX {root_q}\nsetfacl -R -d -m u:{w}:rX {root_q}\n\
                 runuser -u {w} -- test -r {root_q} && echo 'web root readable by {w}'\n\
                 if [ -f {app_q}/.env ]; then runuser -u {w} -- test -r {app_q}/.env && {{ echo '.env is readable by the web server'; exit 1; }} || echo '.env is private'; fi",
                root_q = q(&root),
                app_q = q(&app),
            ),
        ));
    }
    steps.push(Step::new("web server", web_step));
    details.push(match &ctx.cert {
        Some(c) => format!("HTTPS with the installed certificate {c}"),
        None => "HTTPS issued automatically by Caddy (the domain must point here)".into(),
    });

    // 7. Scheduler.
    if site.kind == SiteKind::Laravel && site.scheduler {
        steps.push(Step::new(
            "scheduler",
            format!(
                "{{ crontab -u {user} -l 2>/dev/null | grep -v 'schedule:run' || true; echo '* * * * * cd {app} && php artisan schedule:run >> /dev/null 2>&1'; }} | crontab -u {user} -\n\
                 crontab -u {user} -l | grep -q 'schedule:run' && echo added"
            ),
        ));
        details.push("Laravel scheduler in the site user's crontab".into());
    }

    Ok(Plan {
        op: "site_create",
        title: format!("create site {domain}"),
        secret_scope: Some(domain.clone()),
        risk: Risk::Change,
        details,
        confirm_word: None,
        steps,
        reveal,
    })
}

#[cfg(test)]
mod tests {
    use sesame_core::snapshot::*;

    use super::*;

    fn snap() -> ServerSnapshot {
        ServerSnapshot {
            web_servers: vec![WebServer { kind: WebServerKind::Caddy, version: None, config_path: None }],
            php: vec![PhpInstall { version: "8.3.6".into(), cli: true, fpm_running: true, fpm_pools: vec!["www".into()] }],
            certificates: vec![Certificate {
                path: "/etc/ssl/sesame/response.az/cert.pem".into(),
                subject: None,
                issuer: None,
                names: vec!["*.response.az".into(), "response.az".into()],
                not_after: None,
            }],
            ..Default::default()
        }
    }

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    #[test]
    fn laravel_fpm_site_with_database() {
        let site = NewSite {
            domain: "shop.response.az".into(),
            aliases: vec![],
            kind: SiteKind::Laravel,
            runtime: Runtime::Fpm,
            source: None,
            web_root: "public".into(),
            database: Some(DatabaseKind::Postgres),
            scheduler: true,
        };
        let p = new_site(&site, &snap()).unwrap();
        for s in &p.steps {
            assert!(sh_ok(&s.script), "{}: {}", s.title, s.script);
        }
        let all: String = p.steps.iter().map(|s| s.script.as_str()).collect();
        assert!(all.contains("useradd --create-home --home-dir /home/s_shop_response_az"));
        assert!(all.contains("listen = /run/php/s_shop_response_az.sock"));
        assert!(all.contains("open_basedir] = /home/s_shop_response_az/shop.response.az:/tmp"));
        assert!(all.contains("tls /etc/ssl/sesame/response.az/cert.pem /etc/ssl/sesame/response.az/key.pem"));
        assert!(all.contains("crontab -u s_shop_response_az"));
        assert!(all.contains("setfacl -R -m u:caddy:rX '/home/s_shop_response_az/shop.response.az/public'"));
        assert!(!all.contains("usermod -aG"));
        // A broken config is removed instead of reloaded.
        assert!(all.contains("rm -f /etc/caddy/sites/sesame-shop_response_az.caddy; exit 1"));
        assert_eq!(p.reveal.iter().find(|(k, _)| k == "DB_DATABASE").unwrap().1, "shop_response_az");
    }

    #[test]
    fn remove_site_needs_the_domain_and_backs_up() {
        let p = remove_site("shop.response.az", Some(DatabaseKind::Postgres), &snap()).unwrap();
        assert_eq!(p.confirm_word.as_deref(), Some("shop.response.az"));
        for s in &p.steps {
            assert!(sh_ok(&s.script), "{}", s.title);
        }
        assert!(p.steps[1].script.contains("pg_dump"));
        assert!(p.steps[2].script.contains("userdel --remove s_shop_response_az"));
    }

    #[test]
    fn static_site_from_repo_and_validation() {
        let site = NewSite {
            domain: "sesame.response.az".into(),
            aliases: vec![],
            kind: SiteKind::Static,
            runtime: Runtime::Fpm,
            source: Some(Source { repo: "rasul-chopurov/sesame".into(), branch: "main".into() }),
            web_root: "website".into(),
            database: None,
            scheduler: false,
        };
        let p = new_site(&site, &snap()).unwrap();
        for s in &p.steps {
            assert!(sh_ok(&s.script), "{}", s.title);
        }
        assert!(p.steps[1].script.contains("git clone --quiet --branch 'main' https://github.com/rasul-chopurov/sesame.git"));
        assert!(!p.steps.iter().any(|s| s.title.contains("pool")));
        // A deeper subdomain is not covered by the wildcard.
        let deep = NewSite { domain: "a.b.response.az".into(), ..site.clone() };
        assert!(new_site(&deep, &snap()).unwrap().details.iter().any(|d| d.contains("automatically")));
        let bad = NewSite { web_root: "../etc".into(), ..site.clone() };
        assert!(new_site(&bad, &snap()).is_err());
        let bad = NewSite { domain: "x;rm -rf /".into(), ..site };
        assert!(new_site(&bad, &snap()).is_err());
    }
}

/// Removes a site created by the wizard: web-server file, PHP pool, workers,
/// cron, database (after a backup) and the Linux user with its files.
pub fn remove_site(domain: &str, database: Option<DatabaseKind>, snap: &ServerSnapshot) -> Result<Plan> {
    let domain = domain.trim().to_ascii_lowercase();
    if !valid_domain(&domain) {
        bail!("invalid domain");
    }
    let slug = slug(&domain);
    let user = format!("s_{slug}");
    let home = format!("/home/{user}");
    let ctx = context(snap, &domain);
    let mut steps = vec![Step::new(
        "turn the site off",
        format!(
            "id {user} >/dev/null 2>&1 || {{ echo 'no site user {user}: not created by Sesame'; exit 1; }}\n\
             rm -f /etc/caddy/sites/sesame-{slug}.caddy /etc/frankenphp/Caddyfile.d/sesame-{slug}.caddyfile /etc/nginx/sites-enabled/sesame-{slug}.conf /etc/nginx/sites-available/sesame-{slug}.conf\n\
             for s in caddy frankenphp nginx; do systemctl is-active --quiet $s && systemctl reload $s || true; done\n\
             rm -f /etc/supervisor/conf.d/sesame-{slug}-*.conf\n\
             command -v supervisorctl >/dev/null && {{ supervisorctl reread >/dev/null; supervisorctl update >/dev/null; }} || true\n\
             for f in /etc/php/*/fpm/pool.d/{user}.conf; do [ -f \"$f\" ] && rm -f \"$f\" && v=$(echo \"$f\" | cut -d/ -f4) && systemctl reload php$v-fpm; done\n\
             crontab -u {user} -r 2>/dev/null || true\necho off"
        ),
    )];
    if let Some(engine) = database {
        let db = slug.clone();
        let backup = "umask 077\nmkdir -p /var/backups/sesame && chmod 700 /var/backups/sesame\nf=/var/backups/sesame/{db}-$(date +%Y%m%d-%H%M%S).sql.gz\n".replace("{db}", &db);
        let script = match engine {
            DatabaseKind::Postgres => format!(
                "{backup}cd /tmp\nrunuser -u postgres -- pg_dump '{db}' | gzip > \"$f\"\necho \"backup: $f\"\n\
                 runuser -u postgres -- psql -v ON_ERROR_STOP=1 -q <<'SQL'\nDROP DATABASE IF EXISTS \"{db}\";\nDROP ROLE IF EXISTS \"{db}\";\nSQL"
            ),
            DatabaseKind::MySql | DatabaseKind::MariaDb => format!(
                "{backup}mysqldump --single-transaction '{db}' | gzip > \"$f\"\necho \"backup: $f\"\n\
                 mysql <<'SQL'\nDROP DATABASE IF EXISTS `{db}`;\nDROP USER IF EXISTS '{db}'@'localhost';\nSQL"
            ),
            _ => bail!("unsupported database"),
        };
        steps.push(Step::new(format!("back up and drop {db}"), script));
    }
    steps.push(Step::new(
        format!("remove {user} and {home}"),
        format!("pkill -u {user} 2>/dev/null || true\nsleep 1\nuserdel --remove {user} 2>/dev/null || {{ rm -rf {home}; userdel {user}; }}\ngroupdel {user} 2>/dev/null || true\necho removed"),
    ));
    let _ = ctx;
    Ok(Plan {
        op: "site_remove",
        title: format!("delete site {domain}"),
        risk: Risk::Destructive,
        details: vec![
            format!("{domain} stops being served"),
            format!("{home} and the user {user} are deleted"),
            match database {
                Some(_) => format!("database {slug} is backed up to /var/backups/sesame, then dropped"),
                None => "no database is touched".into(),
            },
        ],
        confirm_word: Some(domain.clone()),
        steps,
        reveal: vec![],
        secret_scope: None,
    })
}

/// The isolated Linux user and app directory of a wizard-made site.
pub fn site_paths(domain: &str) -> (String, String) {
    let slug = slug(domain);
    let user = format!("s_{slug}");
    let app = format!("/home/{user}/{domain}");
    (user, app)
}

/// Writes `KEY=value` pairs into the app's `.env` as its owner. Values come
/// through the heredoc, never through argv.
fn set_env_script(app: &str, pairs: &[(&str, String)]) -> String {
    let mut body = String::from(
        "set_env() { if grep -q \"^$1=\" .env || grep -q \"^# $1=\" .env; then sed -i -E \"s|^#? ?$1=.*|$1=$2|\" .env; else echo \"$1=$2\" >> .env; fi; }\n",
    );
    for (k, v) in pairs {
        body.push_str(&format!("set_env {k} {v}\n"));
    }
    body.push_str("php artisan config:clear >/dev/null 2>&1 || true\necho '.env updated'");
    format!(
        "cd {app_q}\nowner=$(stat -c %U .)\nrunuser -u \"$owner\" -- sh -eu <<'AS_OWNER'\ncd {app_q}\n{body}\nAS_OWNER",
        app_q = q(app)
    )
}

/// Turns a site Sesame created on or off by setting its web-server file
/// aside. The configuration is validated first and rolled back on error;
/// the app, its database and workers are untouched.
pub fn set_site_enabled(domain: &str, enable: bool) -> Result<Plan> {
    if !valid_domain(domain) {
        bail!("invalid domain");
    }
    let slug = slug(domain);
    // Caddy answers hosts without a block with an empty 200, so a turned-off
    // site gets a small 503 block; the real one waits in `<file>.disabled`.
    let caddy = if enable {
        "[ -f \"$f.disabled\" ] || continue\n\
         \x20 mv \"$f.disabled\" \"$f\"\n\
         \x20 if ! $svc validate --config $main --adapter caddyfile >/tmp/sesame-web.log 2>&1; then mv \"$f\" \"$f.disabled\"; placeholder; tail -5 /tmp/sesame-web.log; exit 1; fi"
    } else {
        "[ -f \"$f\" ] && [ ! -f \"$f.disabled\" ] || continue\n\
         \x20 mv \"$f\" \"$f.disabled\"\n\
         \x20 placeholder\n\
         \x20 if ! $svc validate --config $main --adapter caddyfile >/tmp/sesame-web.log 2>&1; then mv \"$f.disabled\" \"$f\"; tail -5 /tmp/sesame-web.log; exit 1; fi"
    };
    let script = format!(
        "placeholder() {{ {{ head -1 \"$f.disabled\"; grep -m1 '^[[:space:]]*tls ' \"$f.disabled\"; printf '\\trespond \"This site is turned off.\" 503\\n}}\\n'; }} > \"$f\"; }}\n\
         done=\n\
         for f in /etc/frankenphp/Caddyfile.d/sesame-{slug}.caddyfile /etc/caddy/sites/sesame-{slug}.caddy; do\n\
         \x20 case \"$f\" in /etc/frankenphp/*) svc=frankenphp; main=/etc/frankenphp/Caddyfile;; *) svc=caddy; main=/etc/caddy/Caddyfile;; esac\n\
         \x20 {caddy}\n\
         \x20 systemctl reload $svc; done=1\n\
         done\n\
         n=sesame-{slug}.conf\n\
         if [ -f /etc/nginx/sites-available/$n ]; then\n\
         \x20 if [ {enable} = true ]; then ln -sf /etc/nginx/sites-available/$n /etc/nginx/sites-enabled/$n; else rm -f /etc/nginx/sites-enabled/$n; fi\n\
         \x20 if ! nginx -t >/tmp/sesame-web.log 2>&1; then tail -5 /tmp/sesame-web.log; if [ {enable} = true ]; then rm -f /etc/nginx/sites-enabled/$n; else ln -sf /etc/nginx/sites-available/$n /etc/nginx/sites-enabled/$n; fi; exit 1; fi\n\
         \x20 systemctl reload nginx; done=1\n\
         fi\n\
         [ -n \"$done\" ] || {{ echo 'nothing to change: only sites created by Sesame can be turned off here'; exit 1; }}\n\
         echo '{msg}'",
        msg = if enable { "site is on" } else { "site is off" },
    );
    Ok(Plan {
        op: if enable { "site_enable" } else { "site_disable" },
        title: format!("turn {domain} {}", if enable { "on" } else { "off" }),
        risk: if enable { Risk::Change } else { Risk::Destructive },
        details: if enable {
            vec!["its web-server file is put back, validated and reloaded".into()]
        } else {
            vec![
                "visitors get “This site is turned off.” (503)".into(),
                "files, database, workers and cron stay — turning it on again is instant".into(),
            ]
        },
        confirm_word: if enable { None } else { Some(domain.to_string()) },
        steps: vec![Step::new(if enable { "turn the site on" } else { "turn the site off" }, script)],
        reveal: vec![],
        secret_scope: None,
    })
}

/// Checks an OpenSSH public key line and returns `type base64` without its comment.
pub fn clean_public_key(key: &str) -> Result<String> {
    let words: Vec<&str> = key.split_whitespace().collect();
    let (Some(kind), Some(b64)) = (words.first(), words.get(1)) else { bail!("paste a public key like `ssh-ed25519 AAAA… you@laptop`") };
    let kinds = ["ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521", "sk-ssh-ed25519@openssh.com", "sk-ecdsa-sha2-nistp256@openssh.com"];
    if !kinds.contains(kind) {
        bail!("unsupported key type {kind}");
    }
    if b64.len() < 60 || !b64.chars().all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)) {
        bail!("this does not look like a public key");
    }
    Ok(format!("{kind} {b64}"))
}

fn valid_person(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Lets a person into the site owner's account with their own SSH key:
/// `sftp` (files only, no shell, no forwarding) or `shell`.
pub fn grant_access(domain: &str, app_path: &str, person: &str, public_key: &str, shell: bool) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    if !valid_person(person) {
        bail!("a name may use letters, digits, dot, dash and underscore");
    }
    let key = clean_public_key(public_key)?;
    let options = if shell { "restrict,pty".to_string() } else { "restrict,command=\\\"$SFTP\\\"".to_string() };
    let script = format!(
        "owner=$(stat -c %U {app})
         home=$(getent passwd \"$owner\" | cut -d: -f6)
         [ \"$owner\" != root ] && [ -n \"$home\" ] || {{ echo 'the site has no owner account'; exit 1; }}
         SFTP=$(sshd -T 2>/dev/null | awk '$1==\"subsystem\" && $2==\"sftp\" {{print $3}}')
         case \"$SFTP\" in /*) ;; *) SFTP=/usr/lib/openssh/sftp-server;; esac
         install -d -o \"$owner\" -g \"$(id -gn \"$owner\")\" -m 700 \"$home/.ssh\"
         f=\"$home/.ssh/authorized_keys\"
         touch \"$f\"
         grep -v ' sesame-access:{person}$' \"$f\" > \"$f.sesame\" || true
         echo \"{options} {key} sesame-access:{person}\" >> \"$f.sesame\"
         chown \"$owner\": \"$f.sesame\" && chmod 600 \"$f.sesame\" && mv \"$f.sesame\" \"$f\"
         echo \"{person} can connect: {how} $owner@$(hostname -f 2>/dev/null || hostname)\"",
        app = q(app_path),
        how = if shell { "ssh" } else { "sftp" },
    );
    Ok(Plan {
        op: "site_access_grant",
        title: format!("{} access to {domain} for {person}", if shell { "SSH" } else { "SFTP" }),
        risk: Risk::Change,
        details: vec![
            format!("{person}'s key is added to the site owner's authorized_keys"),
            if shell { "a shell as the site's user, no port or agent forwarding".into() } else { "files only (SFTP), no shell, no forwarding".into() },
            "only this owner's sites are reachable; other sites stay closed".into(),
        ],
        confirm_word: None,
        steps: vec![Step::new("add the key", script)],
        reveal: vec![],
        secret_scope: None,
    })
}

/// Removes a person's key from the site owner's account.
pub fn revoke_access(domain: &str, app_path: &str, person: &str) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') || !valid_person(person) {
        bail!("invalid input");
    }
    let script = format!(
        "owner=$(stat -c %U {app})
         f=\"$(getent passwd \"$owner\" | cut -d: -f6)/.ssh/authorized_keys\"
         [ -f \"$f\" ] || {{ echo 'no keys'; exit 0; }}
         grep -v ' sesame-access:{person}$' \"$f\" > \"$f.sesame\" || true
         chown \"$owner\": \"$f.sesame\" && chmod 600 \"$f.sesame\" && mv \"$f.sesame\" \"$f\"
         pkill -u \"$owner\" -f 'sftp-server|sshd: '\"$owner\" 2>/dev/null || true
         echo 'access removed'",
        app = q(app_path)
    );
    Ok(Plan {
        op: "site_access_revoke",
        title: format!("remove {person}'s access to {domain}"),
        risk: Risk::Destructive,
        details: vec![format!("{person}'s key is removed; open sessions of the site's user are closed")],
        confirm_word: None,
        steps: vec![Step::new("remove the key", script)],
        reveal: vec![],
        secret_scope: None,
    })
}

/// Outgoing mail for a Laravel site over SMTP (Resend, Stalwart or any relay):
/// MAIL_* in `.env`. SMTP needs no extra composer package.
pub fn mail_for_site(domain: &str, app_path: &str, smtp: &Smtp) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    let safe = |v: &str| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || "._-@+".contains(c));
    if !safe(&smtp.host) || !safe(&smtp.username) || !safe(&smtp.password) || !safe(&smtp.from) {
        bail!("invalid SMTP settings");
    }
    let env = set_env_script(
        app_path,
        &[
            ("MAIL_MAILER", "smtp".into()),
            ("MAIL_HOST", smtp.host.clone()),
            ("MAIL_PORT", smtp.port.to_string()),
            ("MAIL_USERNAME", smtp.username.clone()),
            ("MAIL_PASSWORD", smtp.password.clone()),
            ("MAIL_ENCRYPTION", "tls".into()),
            ("MAIL_SCHEME", "smtp".into()),
            ("MAIL_FROM_ADDRESS", smtp.from.clone()),
        ],
    );
    Ok(Plan {
        op: "site_mail",
        title: format!("mail for {domain} via {}", smtp.host),
        secret_scope: Some(domain.to_string()),
        risk: Risk::Change,
        details: vec![
            format!("sends through {}:{} as {}", smtp.host, smtp.port, smtp.username),
            format!("MAIL_* in the site's .env, from {}", smtp.from),
        ],
        confirm_word: None,
        steps: vec![Step::new("update .env", env)],
        reveal: vec![("MAIL_PASSWORD".into(), smtp.password.clone())],
    })
}

/// An SMTP relay a site sends through.
#[derive(Debug, Clone)]
pub struct Smtp {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
}

/// A Redis ACL user for one site that can only touch keys under `<slug>:`,
/// written to the site's `.env` (REDIS_USERNAME/REDIS_PASSWORD/REDIS_PREFIX).
/// `admin_password` is the server's `requirepass`, when Redis has one.
pub fn redis_for_site(domain: &str, app_path: &str, admin_password: Option<&str>) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    let slug = slug(domain);
    let pw = generate_password();
    let auth = admin_password.map(|p| format!("export REDISCLI_AUTH='{p}'\n")).unwrap_or_default();
    let acl = format!(
        "{auth}( redis-cli --no-auth-warning <<'REDIS'\n\
         ACL SETUSER {slug} reset on >{pw} ~{slug}:* &{slug}:* +@all -@admin -@dangerous\n\
         CONFIG REWRITE\n\
         REDIS\n\
         ) > /tmp/sesame-redis.out 2>&1\n\
         # redis-cli exits 0 on command errors; check the replies.\n\
         if grep -qE 'ERR|NOAUTH|WRONGPASS|NOPERM' /tmp/sesame-redis.out; then cat /tmp/sesame-redis.out; rm -f /tmp/sesame-redis.out; exit 1; fi\n\
         rm -f /tmp/sesame-redis.out\n\
         redis-cli --no-auth-warning ACL LIST | grep -q '^user {slug} ' || {{ echo 'the ACL user was not created'; exit 1; }}\n\
         echo 'acl user {slug} ready'"
    );
    let env = set_env_script(
        app_path,
        &[("REDIS_USERNAME", slug.clone()), ("REDIS_PASSWORD", pw.clone()), ("REDIS_PREFIX", format!("{slug}:"))],
    );
    Ok(Plan {
        op: "site_redis",
        title: format!("Redis for {domain}"),
        secret_scope: Some(domain.to_string()),
        risk: Risk::Change,
        details: vec![
            format!("Redis user {slug} that can only read and write keys starting with {slug}:"),
            "no admin or dangerous commands (FLUSHALL, CONFIG, KEYS…)".into(),
            "REDIS_USERNAME, REDIS_PASSWORD and REDIS_PREFIX are set in the site's .env".into(),
        ],
        confirm_word: None,
        steps: vec![
            Step::new(
                "PHP redis extension",
                "v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;')\n\
                 if php -m | grep -qi '^redis$'; then echo \"php$v-redis already installed\"; else\n\
                 \x20 apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q install php$v-redis >/dev/null\n\
                 \x20 systemctl reload php$v-fpm 2>/dev/null || true\n\
                 \x20 echo \"installed php$v-redis\"\nfi",
            ),
            Step::new("create the ACL user", acl),
            Step::new("update .env", env),
        ],
        reveal: vec![("REDIS_USERNAME".into(), slug.clone()), ("REDIS_PASSWORD".into(), pw), ("REDIS_PREFIX".into(), format!("{slug}:"))],
    })
}

/// A queue worker for one site under supervisor, as the site's owner.
pub fn queue_for_site(domain: &str, app_path: &str, connection: &str, processes: u8) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    if !connection.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || !(1..=8).contains(&processes) {
        bail!("invalid queue settings");
    }
    let slug = slug(domain);
    let program = format!("sesame-{slug}-queue");
    let script = format!(
        "cd {app_q}\nowner=$(stat -c %U .)\nhome=$(getent passwd \"$owner\" | cut -d: -f6)\n\
         cat > /etc/supervisor/conf.d/{program}.conf <<CONF\n\
         [program:{program}]\nprocess_name=%(program_name)s_%(process_num)02d\n\
         command=php {app_path}/artisan queue:work {connection} --sleep=3 --tries=3 --max-time=3600\n\
         directory={app_path}\nuser=$owner\nenvironment=HOME=\"$home\"\nnumprocs={processes}\n\
         autostart=true\nautorestart=true\nstopasgroup=true\nkillasgroup=true\nstopwaitsecs=3600\n\
         redirect_stderr=true\nstdout_logfile={app_path}/storage/logs/queue.log\n\
         CONF\nsupervisorctl reread >/dev/null\nsupervisorctl update\nsupervisorctl status '{program}:*'",
        app_q = q(app_path)
    );
    Ok(Plan {
        op: "site_queue",
        title: format!("queue worker for {domain}"),
        risk: Risk::Change,
        details: vec![format!("{processes} × php artisan queue:work {connection}, as the site's user, restarted automatically")],
        confirm_word: None,
        steps: vec![Step::new("add the supervisor program", script)],
        reveal: vec![],
        secret_scope: None,
    })
}

#[cfg(test)]
mod site_service_tests {
    use super::*;

    fn sh_ok(script: &str) -> bool {
        std::process::Command::new("sh").args(["-n", "-c", script]).status().unwrap().success()
    }

    #[test]
    fn redis_acl_scopes_keys_and_keeps_password_off_argv() {
        let p = redis_for_site("lara.response.az", "/home/s_lara_response_az/lara.response.az", Some("adminpw")).unwrap();
        for s in &p.steps {
            assert!(sh_ok(&s.script), "{}", s.title);
        }
        let acl = &p.steps[1].script;
        assert!(acl.contains("~lara_response_az:*"));
        assert!(acl.contains("-@dangerous"));
        let pw = &p.reveal.iter().find(|(k, _)| k == "REDIS_PASSWORD").unwrap().1;
        // The site password appears only inside heredocs (ACL command, .env writer).
        for line in p.steps.iter().flat_map(|s| s.script.lines()).filter(|l| l.contains(pw.as_str())) {
            assert!(line.starts_with("ACL SETUSER") || line.starts_with("set_env"), "{line}");
        }
    }

    #[test]
    fn queue_worker_runs_as_owner() {
        let p = queue_for_site("lara.response.az", "/home/s_lara_response_az/lara.response.az", "redis", 2).unwrap();
        assert!(sh_ok(&p.steps[0].script));
        assert!(p.steps[0].script.contains("user=$owner"));
        assert!(p.steps[0].script.contains("numprocs=2"));
        assert!(queue_for_site("lara.response.az", "/x", "redis; rm", 1).is_err());
    }
}

/// A database and user for an existing site, written to its `.env`, then migrated.
pub fn database_for_site(domain: &str, app_path: &str, engine: DatabaseKind, laravel: bool) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    let db = slug(domain);
    let mut plan = crate::actions::db_create(engine, &db, &db, None)?;
    plan.op = "site_database";
    plan.title = format!("database for {domain}");
    plan.secret_scope = Some(domain.to_string());
    if laravel {
        let pw = plan.reveal.iter().find(|(k, _)| k == "DB_PASSWORD").map(|(_, v)| v.clone()).unwrap_or_default();
        let (conn, port) = if engine == DatabaseKind::Postgres { ("pgsql", "5432") } else { ("mysql", "3306") };
        let ext = format!(
            "v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;')\n\
             if ! php -m | grep -qi '^pdo_{conn}$'; then apt-get -o DPkg::Lock::Timeout=300 -o Dpkg::Use-Pty=0 -y -q install php$v-{conn} >/dev/null; systemctl reload php$v-fpm 2>/dev/null || true; echo \"installed php$v-{conn}\"; fi"
        );
        plan.steps.insert(0, Step::new("PHP database driver", ext));
        let mut env = set_env_script(
            app_path,
            &[
                ("DB_CONNECTION", conn.into()),
                ("DB_HOST", "127.0.0.1".into()),
                ("DB_PORT", port.into()),
                ("DB_DATABASE", db.clone()),
                ("DB_USERNAME", db.clone()),
                ("DB_PASSWORD", pw),
            ],
        );
        env = env.replace("echo '.env updated'", "echo '.env updated'\nphp artisan migrate --force --no-interaction");
        plan.steps.push(Step::new("write .env and migrate", env));
        plan.details.push("DB_* written to the site's .env, then php artisan migrate".into());
    }
    Ok(plan)
}

/// The Laravel scheduler in the site owner's crontab.
pub fn scheduler_for_site(domain: &str, app_path: &str) -> Result<Plan> {
    if !valid_domain(domain) || !app_path.starts_with('/') || app_path.contains("..") || app_path.contains('\'') {
        bail!("invalid site");
    }
    let script = format!(
        "owner=$(stat -c %U {app_q})\n\
         {{ crontab -u \"$owner\" -l 2>/dev/null | grep -v 'schedule:run' || true; echo '* * * * * cd {app_path} && php artisan schedule:run >> /dev/null 2>&1'; }} | crontab -u \"$owner\" -\n\
         crontab -u \"$owner\" -l | grep -q 'schedule:run' && echo \"scheduler added for $owner\"",
        app_q = q(app_path)
    );
    Ok(Plan {
        op: "site_scheduler",
        title: format!("scheduler for {domain}"),
        risk: Risk::Change,
        details: vec!["php artisan schedule:run every minute, as the site's user".into()],
        confirm_word: None,
        steps: vec![Step::new("add the cron entry", script)],
        reveal: vec![],
        secret_scope: None,
    })
}

#[cfg(test)]
mod more_site_tests {
    use super::*;

    #[test]
    fn site_toggle_validates_and_rolls_back() {
        for on in [true, false] {
            let p = set_site_enabled("shop.example.com", on).unwrap();
            let s = &p.steps[0].script;
            assert!(s.contains("validate --config"));
            assert!(std::process::Command::new("sh").args(["-n", "-c", s]).status().unwrap().success(), "{s}");
        }
        assert_eq!(set_site_enabled("shop.example.com", false).unwrap().confirm_word.as_deref(), Some("shop.example.com"));
        assert!(set_site_enabled("a.com; rm -rf /", false).is_err());
    }

    #[test]
    fn access_keys_are_checked_and_scoped() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGq8qz1fUeQ9h7oX3m2bV4nT6yR0wK8sL5dJ1cP9aZxE anna@mac";
        let p = grant_access("shop.example.com", "/home/s_shop/shop.example.com", "anna", key, false).unwrap();
        let s = &p.steps[0].script;
        assert!(s.contains("restrict,command=\\\"$SFTP\\\""), "{s}");
        assert!(!s.contains("anna@mac"), "comment replaced by our marker");
        assert!(std::process::Command::new("sh").args(["-n", "-c", s]).status().unwrap().success());
        assert!(grant_access("shop.example.com", "/home/s/x", "anna; rm", key, true).is_err());
        assert!(clean_public_key("ssh-ed25519 short").is_err());
        assert!(clean_public_key("ssh-ed25519 AAAA$(reboot)AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_err());
        let r = revoke_access("shop.example.com", "/home/s_shop/shop.example.com", "anna").unwrap();
        assert!(std::process::Command::new("sh").args(["-n", "-c", &r.steps[0].script]).status().unwrap().success());
    }

    #[test]
    fn mail_for_site_writes_smtp_env() {
        let smtp = Smtp { host: "smtp.resend.com".into(), port: 587, username: "resend".into(), password: "re_abc123".into(), from: "hello@lara.response.az".into() };
        let p = mail_for_site("lara.response.az", "/home/s_lara_response_az/lara.response.az", &smtp).unwrap();
        assert!(p.steps[0].script.contains("set_env MAIL_HOST smtp.resend.com"));
        assert_eq!(p.secret_scope.as_deref(), Some("lara.response.az"));
        let bad = Smtp { password: "x'; rm -rf /".into(), ..smtp };
        assert!(mail_for_site("lara.response.az", "/home/s/x", &bad).is_err());
    }

    #[test]
    fn database_and_scheduler_for_site() {
        let p = database_for_site("lara.response.az", "/home/s_lara_response_az/lara.response.az", DatabaseKind::Postgres, true).unwrap();
        assert_eq!(p.secret_scope.as_deref(), Some("lara.response.az"));
        assert!(p.steps.last().unwrap().script.contains("migrate --force"));
        for s in &p.steps {
            let ok = std::process::Command::new("sh").args(["-n", "-c", &s.script]).status().unwrap().success();
            assert!(ok, "{}", s.title);
        }
        let c = scheduler_for_site("lara.response.az", "/home/s_lara_response_az/lara.response.az").unwrap();
        assert!(c.steps[0].script.contains("crontab -u \"$owner\""));
    }
}
