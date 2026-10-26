//! The read-only probe scripts. Each prints `@@SESAME <name>@@` section
//! markers so one SSH round trip collects everything and the parsers can be
//! tested against recorded output.

/// Section marker prefix.
pub const MARK: &str = "@@SESAME ";

/// Phase 1: the whole machine. POSIX sh; every command tolerates absence.
pub const PROBE: &str = r#"
S=""
if [ "$(id -u)" != 0 ] && sudo -n true 2>/dev/null; then S="sudo -n"; fi
sec() { printf '\n@@SESAME %s@@\n' "$1"; }
has() { command -v "$1" >/dev/null 2>&1; }

sec privileged; if [ "$(id -u)" = 0 ] || [ -n "$S" ]; then echo yes; else echo no; fi
sec hostname; hostname 2>/dev/null
sec os_release; cat /etc/os-release 2>/dev/null
sec uname; uname -r; uname -m
sec nproc; nproc 2>/dev/null
sec meminfo; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null
sec df; df -B1 -P / 2>/dev/null | tail -1
sec uptime; cat /proc/uptime 2>/dev/null
sec loadavg; cat /proc/loadavg 2>/dev/null
sec systemd; has systemctl && systemctl list-units --type=service --all --no-legend --plain --no-pager 2>/dev/null
sec supervisor_status; has supervisorctl && $S supervisorctl status 2>/dev/null
sec supervisor_conf
for f in /etc/supervisor/conf.d/*.conf /etc/supervisord.d/*.ini; do
  [ -f "$f" ] && { echo "; file: $f"; $S cat "$f"; echo; }
done
sec ports; has ss && $S ss -tlnpH 2>/dev/null
sec web_bins; for b in frankenphp caddy nginx apache2 httpd; do has $b && echo "$b"; done
sec frankenphp_version; has frankenphp && frankenphp version 2>/dev/null
sec caddy_version; has caddy && caddy version 2>/dev/null
sec nginx_version; has nginx && nginx -v 2>&1
sec apache_version; { has apache2 && apache2 -v 2>/dev/null; } || { has httpd && httpd -v 2>/dev/null; }
sec caddy_config
C=""
if has curl; then C=$(curl -fsS --max-time 3 http://127.0.0.1:2019/config/ 2>/dev/null); fi
if [ -z "$C" ] || [ "$C" = "null" ]; then
  for f in /etc/frankenphp/Caddyfile /etc/caddy/Caddyfile; do
    [ -f "$f" ] || continue
    C=$( { has frankenphp && $S frankenphp adapt --config "$f" 2>/dev/null; } || { has caddy && $S caddy adapt --config "$f" 2>/dev/null; } )
    [ -n "$C" ] && break
  done
fi
printf '%s\n' "$C"
sec caddy_config_path; for f in /etc/frankenphp/Caddyfile /etc/caddy/Caddyfile; do [ -f "$f" ] && echo "$f"; done
sec nginx_dump; has nginx && $S nginx -T 2>/dev/null
sec apache_conf
for f in /etc/apache2/sites-enabled/*.conf /etc/httpd/conf.d/*.conf; do
  [ -f "$f" ] && $S grep -hiE '^[[:space:]]*(<VirtualHost|</VirtualHost|ServerName|ServerAlias|DocumentRoot|SSLCertificateFile|ProxyPass )' "$f"
done
sec php
for p in /usr/bin/php[0-9]* /usr/local/bin/php /usr/bin/php; do
  [ -x "$p" ] && echo "$p $("$p" -r 'echo PHP_VERSION;' 2>/dev/null)"
done
sec php_fpm; ls -1 /etc/php/*/fpm/pool.d/*.conf 2>/dev/null
sec mysql; { has mysql && mysql --version 2>/dev/null; } || { has mariadb && mariadb --version 2>/dev/null; }
sec mysql_dbs; has mysql && $S mysql -NBe 'SHOW DATABASES' 2>/dev/null
sec mysql_sizes; has mysql && $S mysql -NBe 'SELECT table_schema, SUM(data_length + index_length) FROM information_schema.tables GROUP BY table_schema' 2>/dev/null
sec postgres; has psql && psql --version 2>/dev/null
sec postgres_dbs
if has psql; then
  Q="select datname from pg_database where not datistemplate"
  if [ "$(id -u)" = 0 ]; then su - postgres -c "psql -Atc '$Q'" 2>/dev/null
  elif [ -n "$S" ]; then sudo -n -u postgres psql -Atc "$Q" 2>/dev/null; fi
fi
sec postgres_sizes
if has psql; then
  Q="select datname || chr(9) || pg_database_size(datname) from pg_database where not datistemplate"
  if [ "$(id -u)" = 0 ]; then su - postgres -c "psql -Atc '$Q'" 2>/dev/null
  elif [ -n "$S" ]; then sudo -n -u postgres psql -Atc "$Q" 2>/dev/null; fi
fi
sec redis; has redis-server && redis-server --version 2>/dev/null
sec redis_keyspace; has redis-cli && redis-cli --no-auth-warning INFO keyspace 2>/dev/null
sec mongo; has mongod && mongod --version 2>/dev/null | head -1
sec mail; for s in postfix exim4 dovecot stalwart stalwart-mail opendkim rspamd; do systemctl is-active --quiet "$s" 2>/dev/null && echo "$s"; done
sec postfix; has postconf && postconf myhostname mydestination virtual_mailbox_domains 2>/dev/null
sec cron_users
for u in $(cut -d: -f1 /etc/passwd); do
  $S crontab -l -u "$u" 2>/dev/null | sed "s|^|$u	|"
done
sec cron_d; for f in /etc/cron.d/*; do [ -f "$f" ] && sed "s|^|$f	|" "$f"; done
sec letsencrypt; $S ls -1 /etc/letsencrypt/live 2>/dev/null
sec ufw; has ufw && $S ufw status 2>/dev/null
sec fail2ban; has fail2ban-client && $S fail2ban-client status 2>/dev/null
sec docker; has docker && $S docker ps -a --format '{{.Names}}	{{.Image}}	{{.Status}}	{{.Ports}}' 2>/dev/null
sec end
"#;

/// Keys read from an app's `.env`. Passwords and keys are deliberately absent.
pub const ENV_KEYS: &[&str] = &[
    "APP_ENV",
    "APP_URL",
    "DB_CONNECTION",
    "DB_HOST",
    "DB_PORT",
    "DB_DATABASE",
    "DB_USERNAME",
    "CACHE_STORE",
    "CACHE_DRIVER",
    "QUEUE_CONNECTION",
    "SESSION_DRIVER",
    "REDIS_HOST",
    "REDIS_USERNAME",
    "REDIS_PREFIX",
    "MAIL_MAILER",
    "MAIL_HOST",
    "BROADCAST_CONNECTION",
    "BROADCAST_DRIVER",
];

/// Phase 2: details for the app directories and certificates phase 1 found.
pub fn details(app_paths: &[String], cert_paths: &[String]) -> String {
    let mut s = String::from(
        "S=\"\"\nif [ \"$(id -u)\" != 0 ] && sudo -n true 2>/dev/null; then S=\"sudo -n\"; fi\n\
         sec() { printf '\\n@@SESAME %s@@\\n' \"$1\"; }\n",
    );
    let env_re = format!("^({})=", ENV_KEYS.join("|"));
    for p in app_paths {
        let q = quote(p);
        s.push_str(&format!(
            "sec {sec}\n\
             [ -f {q}/artisan ] && echo framework=laravel\n\
             [ -f {q}/wp-config.php ] && echo framework=wordpress\n\
             [ -f {q}/package.json ] && echo framework=node\n\
             [ -f {q}/index.html ] && echo framework=static\n\
             [ -f {q}/composer.lock ] && grep -A3 '\"name\": \"laravel/framework\"' {q}/composer.lock | grep -m1 '\"version\"' | sed 's/^/laravel_version=/'\n\
             [ -f {q}/.env ] && $S grep -E '{env_re}' {q}/.env | sed 's/^/env:/'\n\
             if [ -d {q}/.git ]; then\n\
             \x20 echo \"git_sha=$(git -c safe.directory='*' -C {q} rev-parse --short HEAD 2>/dev/null)\"\n\
             \x20 echo \"git_branch=$(git -c safe.directory='*' -C {q} rev-parse --abbrev-ref HEAD 2>/dev/null)\"\n\
             \x20 echo \"git_remote=$(git -c safe.directory='*' -C {q} config --get remote.origin.url 2>/dev/null)\"\n\
             fi\n\
             [ -f {q}/wp-config.php ] && $S grep -oE \"define\\( *'DB_(NAME|HOST|USER)', *'[^']*'\" {q}/wp-config.php | sed 's/^/wp:/'\n",
            sec = quote(&format!("app:{p}")),
        ));
    }
    for c in cert_paths {
        s.push_str(&format!(
            "sec {sec}\n$S openssl x509 -noout -subject -issuer -enddate -ext subjectAltName -in {q} 2>/dev/null\n",
            sec = quote(&format!("cert:{c}")),
            q = quote(c),
        ));
    }
    s.push_str("sec end\n");
    s
}

/// Single-quotes a value for POSIX sh.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Splits probe output into `(section, body)` pairs, in order.
pub fn sections(output: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = output;
    while let Some(start) = rest.find(MARK) {
        let after = &rest[start + MARK.len()..];
        let Some(end) = after.find("@@") else { break };
        let name = &after[..end];
        let body_start = after[end + 2..].strip_prefix('\n').unwrap_or(&after[end + 2..]);
        let body_end = body_start.find(&format!("\n{MARK}")).unwrap_or(body_start.len());
        out.push((name, &body_start[..body_end]));
        rest = &body_start[body_end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sections() {
        let out = "junk\n@@SESAME a@@\nline1\nline2\n\n@@SESAME b@@\n\n@@SESAME end@@\n";
        assert_eq!(sections(out), vec![("a", "line1\nline2\n"), ("b", ""), ("end", "")]);
    }

    #[test]
    fn scripts_are_valid_posix_sh() {
        let details = details(&["/var/www/it's".into()], &["/etc/ssl/a.pem".into()]);
        for script in [PROBE, details.as_str()] {
            let status = std::process::Command::new("sh")
                .args(["-n", "-c", script])
                .status()
                .unwrap();
            assert!(status.success());
        }
    }

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(quote("/var/www/it's"), r"'/var/www/it'\''s'");
    }
}
