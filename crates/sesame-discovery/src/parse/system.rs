//! Parsers for machine-level sections: OS, services, processes, ports, PHP,
//! databases, mail, cron, firewall, containers.

use sesame_core::snapshot::*;

pub fn os_release(body: &str) -> String {
    let field = |key: &str| {
        body.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
    };
    field("PRETTY_NAME").or_else(|| field("NAME")).unwrap_or_default()
}

/// `/proc/meminfo` lines in kB → (total, available) bytes.
pub fn meminfo(body: &str) -> (u64, u64) {
    let kb = |key: &str| {
        body.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    };
    (kb("MemTotal:"), kb("MemAvailable:"))
}

/// `df -B1 -P /` data line → (total, used) bytes.
pub fn df(body: &str) -> (u64, u64) {
    let cols: Vec<&str> = body.split_whitespace().collect();
    let n = |i: usize| cols.get(i).and_then(|v| v.parse().ok()).unwrap_or(0);
    (n(1), n(2))
}

pub fn uptime(body: &str) -> u64 {
    body.split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v as u64)
        .unwrap_or(0)
}

pub fn loadavg(body: &str) -> [f32; 3] {
    let mut it = body.split_whitespace().map(|v| v.parse().unwrap_or(0.0));
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

/// `systemctl list-units --plain --no-legend`: unit load active sub description…
pub fn systemd(body: &str) -> Vec<SystemService> {
    body.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let unit = it.next()?;
            let load = it.next()?;
            if load != "loaded" {
                return None;
            }
            Some(SystemService {
                unit: unit.to_string(),
                active: it.next()?.to_string(),
                sub: it.next()?.to_string(),
                description: it.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// `supervisorctl status` joined with `[program:x]` sections from the configs.
pub fn supervisor(status: &str, conf: &str) -> Vec<SupervisorProgram> {
    let programs = supervisor_conf(conf);
    status
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let name = it.next()?.to_string();
            let state = it.next()?.to_string();
            let rest = it.collect::<Vec<_>>().join(" ");
            let pid = rest
                .strip_prefix("pid ")
                .and_then(|r| r.split(',').next()?.trim().parse().ok());
            let uptime = rest.split("uptime ").nth(1).map(|u| u.trim().to_string());
            let program = name.split(':').next().unwrap_or(&name);
            let conf = programs.iter().find(|p| p.0 == program);
            let command = conf.and_then(|c| c.1.clone());
            Some(SupervisorProgram {
                role: command.as_deref().map(process_role).unwrap_or_default(),
                command,
                directory: conf.and_then(|c| c.2.clone()),
                user: conf.and_then(|c| c.3.clone()),
                name,
                state,
                pid,
                uptime,
            })
        })
        .collect()
}

type ProgramConf = (String, Option<String>, Option<String>, Option<String>);

fn supervisor_conf(conf: &str) -> Vec<ProgramConf> {
    let mut out: Vec<ProgramConf> = Vec::new();
    for line in conf.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("[program:").and_then(|l| l.strip_suffix(']')) {
            out.push((name.to_string(), None, None, None));
            continue;
        }
        let Some(cur) = out.last_mut() else { continue };
        if line.starts_with('[') {
            // Another section ([group:x], [include]) ends the program.
            out.push((String::new(), None, None, None));
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = Some(v.trim().to_string());
        match k.trim() {
            "command" => cur.1 = v,
            "directory" => cur.2 = v,
            "user" => cur.3 = v,
            _ => {}
        }
    }
    out.retain(|p| !p.0.is_empty());
    out
}

pub fn process_role(command: &str) -> ProcessRole {
    let has = |s: &str| command.contains(s);
    if has("octane:start") || has("octane:frankenphp") || has("octane:swoole") {
        ProcessRole::Octane
    } else if has("horizon") {
        ProcessRole::Horizon
    } else if has("queue:work") || has("queue:listen") {
        ProcessRole::Queue
    } else if has("reverb:start") {
        ProcessRole::Reverb
    } else if has("schedule:work") {
        ProcessRole::Scheduler
    } else if has("node ") || has("npm ") || has("pm2") || has("bun ") {
        ProcessRole::Node
    } else {
        ProcessRole::Other
    }
}

/// `ss -tlnpH`: State Recv-Q Send-Q Local Peer Process. IPv4/IPv6 twins of
/// the same listener are merged.
pub fn ports(body: &str) -> Vec<ListeningPort> {
    let mut out: Vec<ListeningPort> = Vec::new();
    for line in body.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        let Some(local) = cols.get(3) else { continue };
        let Some((addr, port)) = local.rsplit_once(':') else { continue };
        let Ok(port) = port.parse::<u16>() else { continue };
        let addr = addr.split('%').next().unwrap_or(addr).to_string();
        let process = cols
            .get(5)
            .and_then(|p| p.split("((\"").nth(1)?.split('"').next())
            .unwrap_or("")
            .to_string();
        let public = matches!(addr.as_str(), "*" | "0.0.0.0" | "[::]");
        if let Some(existing) = out.iter_mut().find(|p| p.port == port && p.process == process) {
            existing.public |= public;
            if public {
                existing.address = addr;
            }
            continue;
        }
        out.push(ListeningPort { address: addr, port, process, public });
    }
    out.sort_by_key(|p| p.port);
    out
}

/// PHP binaries (`/usr/bin/php8.3 8.3.12`) and FPM pool files.
pub fn php(cli: &str, fpm_pools: &str, services: &[SystemService]) -> Vec<PhpInstall> {
    fn entry(out: &mut Vec<PhpInstall>, services: &[SystemService], version: &str) -> usize {
        let minor = version.split('.').take(2).collect::<Vec<_>>().join(".");
        if let Some(i) = out.iter().position(|p| p.version.starts_with(&minor)) {
            return i;
        }
        out.push(PhpInstall {
            version: version.to_string(),
            cli: false,
            fpm_running: services
                .iter()
                .any(|s| s.unit == format!("php{minor}-fpm.service") && s.active == "active"),
            fpm_pools: Vec::new(),
        });
        out.len() - 1
    }
    let mut out = Vec::new();
    for line in cli.lines() {
        if let Some(v) = line.split_whitespace().nth(1) {
            let i = entry(&mut out, services, v);
            out[i].cli = true;
            if v.len() > out[i].version.len() {
                out[i].version = v.to_string();
            }
        }
    }
    for path in fpm_pools.lines() {
        // /etc/php/8.3/fpm/pool.d/www.conf
        let parts: Vec<&str> = path.split('/').collect();
        if let (Some(v), Some(file)) = (parts.get(3), parts.last()) {
            let i = entry(&mut out, services, v);
            out[i].fpm_pools.push(file.trim_end_matches(".conf").to_string());
        }
    }
    out
}

pub fn databases<'a>(
    get: &dyn Fn(&str) -> &'a str,
    ports: &[ListeningPort],
) -> Vec<DatabaseEngine> {
    let listen = |procs: &[&str]| {
        ports
            .iter()
            .find(|p| procs.contains(&p.process.as_str()))
            .map(|p| format!("{}:{}", p.address, p.port))
    };
    let mut out = Vec::new();

    let mysql = get("mysql").trim();
    let mysql_listen = listen(&["mysqld", "mariadbd"]);
    if !mysql.is_empty() || mysql_listen.is_some() {
        let maria = mysql.contains("MariaDB") || mysql.starts_with("mariadb");
        out.push(DatabaseEngine {
            kind: if maria { DatabaseKind::MariaDb } else { DatabaseKind::MySql },
            version: version_after(mysql, if maria { "Distrib " } else { "Ver " }),
            listen: mysql_listen,
            databases: get("mysql_dbs")
                .lines()
                .map(str::trim)
                .filter(|d| {
                    !d.is_empty()
                        && !matches!(*d, "information_schema" | "performance_schema" | "mysql" | "sys")
                })
                .map(String::from)
                .collect(),
        });
    }

    let pg = get("postgres").trim();
    let pg_listen = listen(&["postgres", "postmaster"]);
    if !pg.is_empty() || pg_listen.is_some() {
        out.push(DatabaseEngine {
            kind: DatabaseKind::Postgres,
            version: pg.split_whitespace().last().map(String::from),
            listen: pg_listen,
            databases: get("postgres_dbs")
                .lines()
                .map(str::trim)
                .filter(|d| !d.is_empty() && *d != "postgres")
                .map(String::from)
                .collect(),
        });
    }

    let redis = get("redis").trim();
    let redis_listen = listen(&["redis-server"]);
    if !redis.is_empty() || redis_listen.is_some() {
        out.push(DatabaseEngine {
            kind: DatabaseKind::Redis,
            version: version_after(redis, "v="),
            listen: redis_listen,
            databases: get("redis_keyspace")
                .lines()
                .filter_map(|l| {
                    let (db, rest) = l.trim().split_once(':')?;
                    let keys = rest.split(',').next()?.strip_prefix("keys=")?;
                    db.starts_with("db").then(|| format!("{db} ({keys} keys)"))
                })
                .collect(),
        });
    }

    let mongo = get("mongo").trim();
    let mongo_listen = listen(&["mongod"]);
    if !mongo.is_empty() || mongo_listen.is_some() {
        out.push(DatabaseEngine {
            kind: DatabaseKind::MongoDb,
            version: version_after(mongo, "version v"),
            listen: mongo_listen,
            databases: Vec::new(),
        });
    }
    out
}

fn version_after(s: &str, marker: &str) -> Option<String> {
    let rest = s.split(marker).nth(1)?;
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .collect();
    (!v.is_empty()).then_some(v)
}

pub fn mail(active: &str, postfix: &str) -> MailInfo {
    let get = |key: &str| {
        postfix
            .lines()
            .find_map(|l| l.strip_prefix(key)?.trim_start().strip_prefix('='))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let mut domains: Vec<String> = Vec::new();
    for key in ["mydestination", "virtual_mailbox_domains"] {
        for d in get(key).unwrap_or_default().split([',', ' ']) {
            let d = d.trim();
            if !d.is_empty() && !d.starts_with('$') && d != "localhost" && !domains.iter().any(|x| x == d) {
                domains.push(d.to_string());
            }
        }
    }
    MailInfo {
        servers: active.lines().map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
        hostname: get("myhostname"),
        domains,
        external: Vec::new(),
    }
}

/// User crontabs (`user<TAB>line`) and `/etc/cron.d` files (`path<TAB>line`,
/// where the line carries its own user field).
pub fn cron(users: &str, cron_d: &str) -> Vec<CronEntry> {
    let mut out = Vec::new();
    let mut push = |source: &str, user: Option<&str>, line: &str| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let first = line.split_whitespace().next().unwrap_or("");
        if !first.starts_with('@') && first.contains('=') {
            return; // MAILTO=, PATH=
        }
        let n = if first.starts_with('@') { 1 } else { 5 };
        let mut parts = line.split_whitespace();
        let schedule: Vec<&str> = parts.by_ref().take(n).collect();
        if schedule.len() < n {
            return;
        }
        let (user, command) = match user {
            Some(u) => (u.to_string(), parts.collect::<Vec<_>>().join(" ")),
            None => {
                let u = parts.next().unwrap_or("").to_string();
                (u, parts.collect::<Vec<_>>().join(" "))
            }
        };
        if command.is_empty() {
            return;
        }
        out.push(CronEntry {
            user,
            schedule: schedule.join(" "),
            command,
            source: source.to_string(),
        });
    };
    for l in users.lines() {
        if let Some((user, line)) = l.split_once('\t') {
            push("crontab", Some(user), line);
        }
    }
    for l in cron_d.lines() {
        if let Some((file, line)) = l.split_once('\t') {
            push(file, None, line);
        }
    }
    out
}

pub fn firewall(ufw: &str, fail2ban: &str) -> FirewallInfo {
    FirewallInfo {
        ufw: ufw
            .lines()
            .find_map(|l| l.strip_prefix("Status:"))
            .map(|s| s.trim().to_string()),
        fail2ban_jails: fail2ban
            .lines()
            .find_map(|l| l.split("Jail list:").nth(1))
            .map(|l| l.split(',').map(|j| j.trim().to_string()).filter(|j| !j.is_empty()).collect())
            .unwrap_or_default(),
    }
}

pub fn containers(body: &str) -> Vec<Container> {
    body.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some(Container {
                name: it.next()?.to_string(),
                image: it.next()?.to_string(),
                status: it.next().unwrap_or("").to_string(),
                ports: it.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_joins_status_and_config() {
        let status = "shop-octane                     RUNNING   pid 555630, uptime 7:47:43\n\
                      shop-queue:shop-queue_00   RUNNING   pid 575035, uptime 0:47:29\n\
                      worker                               FATAL     Exited too quickly (process log may have details)\n";
        let conf = "; file: /etc/supervisor/conf.d/octane.conf\n[program:shop-octane]\n\
                    command=php /var/www/shop/artisan octane:start --server=frankenphp\n\
                    directory=/var/www/shop\nuser=shop\n\n\
                    [program:shop-queue]\nprocess_name=%(program_name)s_%(process_num)02d\n\
                    command=php /var/www/shop/artisan queue:work redis\nuser=www-data\n";
        let p = supervisor(status, conf);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].role, ProcessRole::Octane);
        assert_eq!(p[0].pid, Some(555630));
        assert_eq!(p[0].uptime.as_deref(), Some("7:47:43"));
        assert_eq!(p[0].directory.as_deref(), Some("/var/www/shop"));
        assert_eq!(p[1].role, ProcessRole::Queue);
        assert_eq!(p[1].user.as_deref(), Some("www-data"));
        assert_eq!(p[2].state, "FATAL");
        assert_eq!(p[2].pid, None);
    }

    #[test]
    fn ports_merge_ipv4_and_ipv6() {
        let body = "LISTEN 0 4096 127.0.0.1:3306 0.0.0.0:* users:((\"mysqld\",pid=1,fd=36))\n\
                    LISTEN 0 511 127.0.0.1:6379 0.0.0.0:* users:((\"redis-server\",pid=2,fd=6))\n\
                    LISTEN 0 511 [::1]:6379 [::]:* users:((\"redis-server\",pid=2,fd=7))\n\
                    LISTEN 0 4096 *:8000 *:* users:((\"frankenphp\",pid=3,fd=8))\n\
                    LISTEN 0 4096 127.0.0.53%lo:53 0.0.0.0:* users:((\"systemd-resolve\",pid=4,fd=15))\n";
        let p = ports(body);
        assert_eq!(p.iter().map(|p| p.port).collect::<Vec<_>>(), vec![53, 3306, 6379, 8000]);
        assert_eq!(p[0].address, "127.0.0.53");
        assert!(p[3].public && !p[2].public);
        assert_eq!(p[1].process, "mysqld");
    }

    #[test]
    fn cron_reads_users_and_cron_d() {
        let users = "shop\t# comment\nshop\tMAILTO=\"\"\nshop\t* * * * * cd /var/www/app && php artisan schedule:run >> /dev/null 2>&1\nroot\t@daily /usr/local/bin/backup\n";
        let d = "/etc/cron.d/certbot\t0 */12 * * * root test -x /usr/bin/certbot && certbot -q renew\n";
        let c = cron(users, d);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].user, "shop");
        assert_eq!(c[0].schedule, "* * * * *");
        assert!(c[0].command.contains("schedule:run"));
        assert_eq!(c[1].schedule, "@daily");
        assert_eq!(c[2].user, "root");
        assert_eq!(c[2].source, "/etc/cron.d/certbot");
    }

    #[test]
    fn php_versions_and_pools() {
        let services = vec![SystemService {
            unit: "php8.5-fpm.service".into(),
            active: "active".into(),
            sub: "running".into(),
            description: String::new(),
        }];
        let p = php(
            "/usr/bin/php8.5 8.5.11\n/usr/bin/php 8.5.11\n",
            "/etc/php/8.5/fpm/pool.d/www.conf\n",
            &services,
        );
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].version, "8.5.11");
        assert!(p[0].cli && p[0].fpm_running);
        assert_eq!(p[0].fpm_pools, vec!["www"]);
    }

    #[test]
    fn mysql_and_redis_versions() {
        let ports = ports("LISTEN 0 1 127.0.0.1:3306 0.0.0.0:* users:((\"mysqld\",pid=1,fd=1))\n");
        let get = |name: &str| -> &'static str {
            match name {
                "mysql" => "mysql  Ver 8.0.46-0ubuntu0.24.04.4 for Linux on aarch64 ((Ubuntu))",
                "mysql_dbs" => "information_schema\nmysql\nshop\nperformance_schema\nsys\n",
                "redis" => "Redis server v=7.0.15 sha=00000000:0 malloc=jemalloc-5.3.0 bits=64",
                "redis_keyspace" => "# Keyspace\r\ndb0:keys=1520,expires=12,avg_ttl=0\r\n",
                _ => "",
            }
        };
        let dbs = databases(&get, &ports);
        assert_eq!(dbs.len(), 2);
        assert_eq!(dbs[0].kind, DatabaseKind::MySql);
        assert_eq!(dbs[0].version.as_deref(), Some("8.0.46-0ubuntu0.24.04.4"));
        assert_eq!(dbs[0].databases, vec!["shop"]);
        assert_eq!(dbs[0].listen.as_deref(), Some("127.0.0.1:3306"));
        assert_eq!(dbs[1].version.as_deref(), Some("7.0.15"));
        assert_eq!(dbs[1].databases, vec!["db0 (1520 keys)"]);
    }
}
