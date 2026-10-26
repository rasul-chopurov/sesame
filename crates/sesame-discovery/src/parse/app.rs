//! Phase 2 parsers: an app directory and a certificate.

use sesame_core::snapshot::{Access, AppEnv, Certificate, Framework, GitInfo};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

#[derive(Debug, Default, PartialEq)]
pub struct AppDetails {
    pub framework: Framework,
    pub version: Option<String>,
    pub env: Option<AppEnv>,
    pub git: Option<GitInfo>,
    pub owner: Option<String>,
    pub access: Vec<Access>,
}

/// `restrict,pty ssh-ed25519 AAAA… sesame-access:anna` → anna, shell.
pub fn access_line(line: &str) -> Option<Access> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let name = words.last()?.strip_prefix("sesame-access:")?.to_string();
    let ki = words.iter().position(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-"))?;
    let options = if ki > 0 { words[..ki].join(" ") } else { String::new() };
    let mode = if options.split(',').any(|o| o == "pty") { "shell" } else { "sftp" };
    let material = words.get(ki + 1).copied().unwrap_or("");
    let tail: String = material.chars().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect();
    Some(Access { name, mode: mode.into(), key: format!("{} …{tail}", words[ki]) })
}

pub fn app(body: &str) -> AppDetails {
    let mut d = AppDetails::default();
    let mut env = AppEnv::default();
    let mut has_env = false;
    for line in body.lines() {
        if let Some(f) = line.strip_prefix("framework=") {
            // First match wins: artisan beats package.json beats index.html.
            if d.framework == Framework::Unknown {
                d.framework = match f {
                    "laravel" => Framework::Laravel,
                    "wordpress" => Framework::WordPress,
                    "node" => Framework::Node,
                    "static" => Framework::Static,
                    _ => Framework::Unknown,
                };
            }
        } else if let Some(v) = line.strip_prefix("laravel_version=") {
            // `            "version": "v12.3.0",`
            d.version = v.split('"').nth(3).map(|v| v.trim_start_matches('v').to_string());
        } else if let Some(kv) = line.strip_prefix("env:") {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let v = Some(unquote(v).to_string()).filter(|v| !v.is_empty());
            has_env = true;
            let slot = match k {
                "APP_ENV" => &mut env.app_env,
                "APP_URL" => &mut env.app_url,
                "DB_CONNECTION" => &mut env.db_connection,
                "DB_HOST" => &mut env.db_host,
                "DB_PORT" => &mut env.db_port,
                "DB_DATABASE" => &mut env.db_database,
                "DB_USERNAME" => &mut env.db_username,
                "CACHE_STORE" | "CACHE_DRIVER" => &mut env.cache_store,
                "QUEUE_CONNECTION" => &mut env.queue_connection,
                "SESSION_DRIVER" => &mut env.session_driver,
                "REDIS_HOST" => &mut env.redis_host,
                "REDIS_USERNAME" => &mut env.redis_username,
                "REDIS_PREFIX" => &mut env.redis_prefix,
                "MAIL_MAILER" => &mut env.mail_mailer,
                "MAIL_HOST" => &mut env.mail_host,
                "BROADCAST_CONNECTION" | "BROADCAST_DRIVER" => &mut env.broadcast_connection,
                _ => continue,
            };
            *slot = v;
        } else if let Some(v) = line.strip_prefix("git_sha=") {
            if !v.is_empty() {
                d.git.get_or_insert_with(GitInfo::default).sha = v.to_string();
            }
        } else if let Some(v) = line.strip_prefix("git_branch=") {
            if let Some(g) = d.git.as_mut() {
                g.branch = Some(v.to_string()).filter(|b| !b.is_empty() && b != "HEAD");
            }
        } else if let Some(v) = line.strip_prefix("git_remote=") {
            if let Some(g) = d.git.as_mut() {
                g.remote = Some(v.to_string()).filter(|r| !r.is_empty());
                g.github_repo = GitInfo::github_repo_of(v);
            }
        } else if let Some(o) = line.strip_prefix("owner=") {
            d.owner = Some(o.to_string()).filter(|o| !o.is_empty());
        } else if let Some(a) = line.strip_prefix("access:") {
            d.access.extend(access_line(a));
        } else if let Some(wp) = line.strip_prefix("wp:") {
            // define( 'DB_NAME', 'wordpress'
            let parts: Vec<&str> = wp.split('\'').collect();
            let (Some(k), Some(v)) = (parts.get(1), parts.get(3)) else { continue };
            has_env = true;
            env.db_connection.get_or_insert_with(|| "mysql".into());
            let v = Some(v.to_string());
            match *k {
                "DB_NAME" => env.db_database = v,
                "DB_HOST" => env.db_host = v,
                "DB_USER" => env.db_username = v,
                _ => {}
            }
        }
    }
    if has_env {
        d.env = Some(env);
    }
    d
}

fn unquote(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(v)
}

/// `openssl x509 -noout -subject -issuer -enddate -ext subjectAltName`.
pub fn certificate(path: &str, body: &str) -> Option<Certificate> {
    if body.trim().is_empty() {
        return None;
    }
    let field = |key: &str| {
        body.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_string())
    };
    let names = body
        .lines()
        .skip_while(|l| !l.contains("Subject Alternative Name"))
        .nth(1)
        .map(|l| {
            l.split(',')
                .filter_map(|n| n.trim().strip_prefix("DNS:").map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Some(Certificate {
        path: path.to_string(),
        subject: field("subject="),
        issuer: field("issuer="),
        names,
        not_after: field("notAfter=").and_then(|d| openssl_date(&d)),
    })
}

/// `Oct  1 12:00:00 2041 GMT` → RFC 3339.
fn openssl_date(s: &str) -> Option<String> {
    let norm = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let fmt = format_description!(
        "[month repr:short] [day padding:none] [hour]:[minute]:[second] [year] GMT"
    );
    let dt = time::PrimitiveDateTime::parse(&norm, fmt).ok()?;
    dt.assume_utc().format(&Rfc3339).ok()
}

pub fn now_rfc3339() -> String {
    let now = OffsetDateTime::now_utc();
    now.replace_nanosecond(0).unwrap_or(now).format(&Rfc3339).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laravel_app_without_secrets() {
        let body = "framework=laravel\nframework=node\nlaravel_version=            \"version\": \"v12.3.0\",\n\
                    env:APP_ENV=production\nenv:APP_URL=\"https://example.com\"\nenv:DB_CONNECTION=mysql\n\
                    env:DB_HOST=127.0.0.1\nenv:DB_DATABASE=shop\nenv:QUEUE_CONNECTION=redis\nenv:MAIL_MAILER=log\n";
        let d = app(body);
        assert_eq!(d.framework, Framework::Laravel);
        assert_eq!(d.version.as_deref(), Some("12.3.0"));
        let env = d.env.unwrap();
        assert_eq!(env.app_url.as_deref(), Some("https://example.com"));
        assert_eq!(env.db_database.as_deref(), Some("shop"));
        assert_eq!(env.queue_connection.as_deref(), Some("redis"));
    }

    #[test]
    fn git_checkout() {
        let a = app("owner=s_shop\naccess:restrict,pty ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabcdefghij0123456789 sesame-access:anna\naccess:restrict,command=\"/usr/lib/openssh/sftp-server\" ssh-ed25519 AAAAC3Nza sesame-access:dev.bob\n");
        assert_eq!(a.owner.as_deref(), Some("s_shop"));
        assert_eq!((a.access[0].name.as_str(), a.access[0].mode.as_str()), ("anna", "shell"));
        assert_eq!((a.access[1].name.as_str(), a.access[1].mode.as_str()), ("dev.bob", "sftp"));
        let d = app("framework=laravel\ngit_sha=1a2b3c4\ngit_branch=main\ngit_remote=git@github.com:rasul-chopurov/shop.git\n");
        let g = d.git.unwrap();
        assert_eq!(g.sha, "1a2b3c4");
        assert_eq!(g.branch.as_deref(), Some("main"));
        assert_eq!(g.github_repo.as_deref(), Some("rasul-chopurov/shop"));
        assert_eq!(GitInfo::github_repo_of("https://github.com/a/b"), Some("a/b".into()));
        assert_eq!(GitInfo::github_repo_of("git@gitlab.com:a/b.git"), None);
    }

    #[test]
    fn wordpress_db_from_wp_config() {
        let d = app("framework=wordpress\nwp:define( 'DB_NAME', 'blog'\nwp:define( 'DB_USER', 'blog_user'\n");
        assert_eq!(d.framework, Framework::WordPress);
        let env = d.env.unwrap();
        assert_eq!(env.db_database.as_deref(), Some("blog"));
        assert_eq!(env.db_connection.as_deref(), Some("mysql"));
    }

    #[test]
    fn parses_openssl_output() {
        let body = "subject=CN = CloudFlare Origin Certificate, O = \"CloudFlare, Inc.\"\n\
                    issuer=C = US, O = \"CloudFlare, Inc.\", OU = CloudFlare Origin SSL Certificate Authority\n\
                    notAfter=Oct  1 12:00:00 2041 GMT\nX509v3 Subject Alternative Name: \n    DNS:*.example.com, DNS:example.com\n";
        let c = certificate("/etc/ssl/shop/origin.pem", body).unwrap();
        assert_eq!(c.names, vec!["*.example.com", "example.com"]);
        assert_eq!(c.not_after.as_deref(), Some("2041-10-01T12:00:00Z"));
        assert!(c.issuer.unwrap().contains("CloudFlare"));
    }
}
