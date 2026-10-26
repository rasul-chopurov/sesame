//! Web server configs → sites. Caddy/FrankenPHP comes as JSON (admin API or
//! `caddy adapt`), Nginx as `nginx -T`, Apache as grepped vhost lines.

use serde_json::Value;
use sesame_core::snapshot::{Site, WebServerKind};

/// Sites from a Caddy JSON config. Each top-level route with a host matcher
/// is a site; handlers are walked recursively through subroutes.
pub fn caddy(config: &str, kind: WebServerKind) -> Vec<Site> {
    let Ok(json) = serde_json::from_str::<Value>(config.trim()) else {
        return Vec::new();
    };
    let tls_files: Vec<String> = json
        .pointer("/apps/tls/certificates/load_files")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("certificate")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut sites: Vec<Site> = Vec::new();
    let servers = json.pointer("/apps/http/servers").and_then(Value::as_object);
    for server in servers.into_iter().flat_map(|s| s.values()) {
        let https = server
            .get("listen")
            .and_then(Value::as_array)
            .is_some_and(|l| l.iter().any(|a| a.as_str().is_some_and(|a| a.ends_with(":443"))));
        for route in server.get("routes").and_then(Value::as_array).into_iter().flatten() {
            let domains = hosts(route);
            let mut site = Site {
                domains: if domains.is_empty() { vec!["*".into()] } else { domains },
                served_by: Some(kind),
                ..Default::default()
            };
            walk_handlers(route, &mut site);
            if site.root.is_none() && site.upstreams.is_empty() && !site.php {
                continue; // redirects, plain `respond` routes
            }
            if https && site.domains != ["*"] {
                site.tls = Some(tls_files.first().cloned().unwrap_or_else(|| "automatic".into()));
            }
            merge(&mut sites, site);
        }
    }
    sites
}

fn hosts(route: &Value) -> Vec<String> {
    route
        .get("match")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("host")?.as_array())
        .flatten()
        .filter_map(|h| h.as_str().map(String::from))
        .collect()
}

fn walk_handlers(node: &Value, site: &mut Site) {
    for h in node.get("handle").and_then(Value::as_array).into_iter().flatten() {
        match h.get("handler").and_then(Value::as_str) {
            Some("vars") | Some("file_server") => {
                if let Some(root) = h.get("root").and_then(Value::as_str) {
                    site.root.get_or_insert_with(|| root.to_string());
                }
            }
            Some("php") | Some("php_server") => site.php = true,
            Some("reverse_proxy") => {
                for u in h.get("upstreams").and_then(Value::as_array).into_iter().flatten() {
                    if let Some(dial) = u.get("dial").and_then(Value::as_str) {
                        if !site.upstreams.iter().any(|x| x == dial) {
                            site.upstreams.push(dial.to_string());
                        }
                    }
                }
                // php_fastcgi is a reverse_proxy with a fastcgi transport.
                if h.pointer("/transport/protocol").and_then(Value::as_str) == Some("fastcgi") {
                    site.php = true;
                }
            }
            Some("subroute") => {
                for r in h.get("routes").and_then(Value::as_array).into_iter().flatten() {
                    walk_handlers(r, site);
                }
            }
            _ => {}
        }
    }
}

/// Folds a vhost into an existing site with an overlapping domain (port 80
/// redirect + port 443 block are one site).
fn merge(sites: &mut Vec<Site>, site: Site) {
    if let Some(existing) = sites
        .iter_mut()
        .find(|s| s.domains.iter().any(|d| d != "*" && site.domains.contains(d)))
    {
        for d in site.domains {
            if !existing.domains.contains(&d) {
                existing.domains.push(d);
            }
        }
        if existing.root.is_none() {
            existing.root = site.root;
        }
        for u in site.upstreams {
            if !existing.upstreams.contains(&u) {
                existing.upstreams.push(u);
            }
        }
        existing.php |= site.php;
        if existing.tls.is_none() {
            existing.tls = site.tls;
        }
        return;
    }
    sites.push(site);
}

/// One directive of an nginx config: `name args… ;` or `name args… { … }`.
#[derive(Debug)]
struct Directive {
    name: String,
    args: Vec<String>,
    block: Option<Vec<Directive>>,
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '#' => {
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
            }
            '{' | '}' | ';' => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
                tokens.push(c.to_string());
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn parse_block(tokens: &[String], pos: &mut usize) -> Vec<Directive> {
    let mut out = Vec::new();
    let mut words: Vec<String> = Vec::new();
    while *pos < tokens.len() {
        let t = &tokens[*pos];
        *pos += 1;
        match t.as_str() {
            ";" => {
                if !words.is_empty() {
                    let name = words.remove(0);
                    out.push(Directive { name, args: std::mem::take(&mut words), block: None });
                }
            }
            "{" => {
                let block = parse_block(tokens, pos);
                let name = if words.is_empty() { String::new() } else { words.remove(0) };
                out.push(Directive { name, args: std::mem::take(&mut words), block: Some(block) });
            }
            "}" => return out,
            _ => words.push(t.clone()),
        }
    }
    out
}

fn collect_servers<'a>(dirs: &'a [Directive], out: &mut Vec<&'a [Directive]>) {
    for d in dirs {
        if let Some(block) = &d.block {
            if d.name == "server" {
                out.push(block);
            } else if d.name != "stream" {
                collect_servers(block, out);
            }
        }
    }
}

fn find_all<'a>(dirs: &'a [Directive], name: &str, out: &mut Vec<&'a Directive>) {
    for d in dirs {
        if d.name == name {
            out.push(d);
        }
        if let Some(block) = &d.block {
            find_all(block, name, out);
        }
    }
}

/// Sites from `nginx -T` output (all files concatenated, includes not inlined).
pub fn nginx(dump: &str) -> Vec<Site> {
    let tokens = tokenize(dump);
    let mut pos = 0;
    let tree = parse_block(&tokens, &mut pos);
    let mut servers = Vec::new();
    collect_servers(&tree, &mut servers);

    let mut sites = Vec::new();
    for server in servers {
        let first = |name: &str| {
            server
                .iter()
                .find(|d| d.name == name && d.block.is_none())
                .and_then(|d| d.args.first().cloned())
        };
        let domains: Vec<String> = server
            .iter()
            .filter(|d| d.name == "server_name")
            .flat_map(|d| d.args.iter().cloned())
            .filter(|n| n != "_" && !n.is_empty())
            .collect();
        let mut site = Site {
            domains: if domains.is_empty() { vec!["*".into()] } else { domains },
            root: first("root"),
            served_by: Some(WebServerKind::Nginx),
            tls: first("ssl_certificate"),
            ..Default::default()
        };
        let mut found = Vec::new();
        find_all(server, "proxy_pass", &mut found);
        for d in &found {
            if let Some(u) = d.args.first() {
                let u = u.trim_start_matches("http://").trim_start_matches("https://").trim_end_matches('/');
                if !site.upstreams.iter().any(|x| x == u) {
                    site.upstreams.push(u.to_string());
                }
            }
        }
        let mut fcgi = Vec::new();
        find_all(server, "fastcgi_pass", &mut fcgi);
        site.php = !fcgi.is_empty();
        if site.root.is_none() {
            let mut roots = Vec::new();
            find_all(server, "root", &mut roots);
            site.root = roots.first().and_then(|d| d.args.first().cloned());
        }
        if site.root.is_none() && site.upstreams.is_empty() && !site.php {
            continue; // bare `return 301` blocks
        }
        merge(&mut sites, site);
    }
    sites
}

/// Sites from grepped Apache vhost lines.
pub fn apache(lines: &str) -> Vec<Site> {
    let mut sites = Vec::new();
    let mut cur: Option<Site> = None;
    for line in lines.lines().map(str::trim) {
        let lower = line.to_ascii_lowercase();
        let arg = || line.split_whitespace().nth(1).map(|s| s.trim_matches('"').to_string());
        if lower.starts_with("<virtualhost") {
            cur = Some(Site { served_by: Some(WebServerKind::Apache), ..Default::default() });
        } else if lower.starts_with("</virtualhost") {
            if let Some(mut s) = cur.take() {
                if s.domains.is_empty() {
                    s.domains.push("*".into());
                }
                merge(&mut sites, s);
            }
        } else if let Some(s) = cur.as_mut() {
            if lower.starts_with("servername") || lower.starts_with("serveralias") {
                s.domains.extend(line.split_whitespace().skip(1).map(String::from));
            } else if lower.starts_with("documentroot") {
                s.root = arg();
            } else if lower.starts_with("sslcertificatefile") {
                s.tls = arg();
            } else if lower.starts_with("proxypass ") {
                if let Some(u) = line.split_whitespace().nth(2) {
                    s.upstreams.push(u.trim_start_matches("http://").trim_end_matches('/').to_string());
                }
            }
        }
    }
    sites
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frankenphp adapt` output for a Caddyfile like shop's
    /// `deploy/Caddyfile.octane` plus the stock `http://` block.
    const CADDY: &str = r#"{
      "apps": {
        "frankenphp": {},
        "http": {"servers": {
          "srv0": {"listen": [":443"], "routes": [{
            "match": [{"host": ["new.example.com", "example.com"]}],
            "handle": [{"handler": "subroute", "routes": [
              {"handle": [{"handler": "vars", "root": "/var/www/shop/public"}]},
              {"handle": [{"handler": "headers"}], "match": [{"path": ["/build/*"]}]},
              {"handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:8091"}]}], "match": [{"path": ["/app/*"]}]},
              {"handle": [{"handler": "file_server", "hide": ["/etc/frankenphp/Caddyfile"]}]},
              {"handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": "127.0.0.1:8000"}]}]}
            ]}],
            "terminal": true
          }]},
          "srv1": {"listen": [":80"], "routes": [{
            "handle": [{"handler": "subroute", "routes": [
              {"handle": [{"handler": "vars", "root": "/usr/share/frankenphp/"}]},
              {"handle": [{"handler": "php", "split_path": [".php"]}]}
            ]}]
          }]}
        }},
        "tls": {"certificates": {"load_files": [
          {"certificate": "/etc/ssl/shop/origin.pem", "key": "/etc/ssl/shop/origin.key"}
        ]}}
      }
    }"#;

    #[test]
    fn caddy_sites_from_adapted_json() {
        let sites = caddy(CADDY, WebServerKind::FrankenPhp);
        assert_eq!(sites.len(), 2);
        let s = &sites[0];
        assert_eq!(s.domains, vec!["new.example.com", "example.com"]);
        assert_eq!(s.root.as_deref(), Some("/var/www/shop/public"));
        assert_eq!(s.upstreams, vec!["127.0.0.1:8091", "127.0.0.1:8000"]);
        assert_eq!(s.tls.as_deref(), Some("/etc/ssl/shop/origin.pem"));
        assert_eq!(sites[1].domains, vec!["*"]);
        assert!(sites[1].php);
        assert_eq!(sites[1].tls, None);
    }

    const NGINX: &str = r#"
# configuration file /etc/nginx/nginx.conf:
http {
    include /etc/nginx/sites-enabled/*;
}
# configuration file /etc/nginx/sites-enabled/shop.test:
server {
    listen 80;
    server_name shop.example.com www.shop.example.com;
    return 301 https://$host$request_uri;
}
server {
    listen 443 ssl http2;
    server_name shop.example.com www.shop.example.com;
    root /home/forge/shop.example.com/public;
    ssl_certificate /etc/nginx/ssl/shop/fullchain.pem; # managed
    location ~ \.php$ {
        fastcgi_pass unix:/var/run/php/php8.3-fpm.sock;
    }
}
# configuration file /etc/nginx/sites-enabled/api:
server {
    listen 80;
    server_name "api.example.com";
    location / { proxy_pass http://127.0.0.1:3000/; }
}
"#;

    #[test]
    fn nginx_sites_merge_redirect_and_tls_blocks() {
        let sites = nginx(NGINX);
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].domains, vec!["shop.example.com", "www.shop.example.com"]);
        assert_eq!(sites[0].root.as_deref(), Some("/home/forge/shop.example.com/public"));
        assert!(sites[0].php);
        assert_eq!(sites[0].tls.as_deref(), Some("/etc/nginx/ssl/shop/fullchain.pem"));
        assert_eq!(sites[1].domains, vec!["api.example.com"]);
        assert_eq!(sites[1].upstreams, vec!["127.0.0.1:3000"]);
    }

    #[test]
    fn apache_vhosts() {
        let lines = "<VirtualHost *:443>\n  ServerName blog.example.com\n  ServerAlias www.blog.example.com\n  DocumentRoot \"/var/www/blog\"\n  SSLCertificateFile /etc/letsencrypt/live/blog.example.com/fullchain.pem\n</VirtualHost>\n";
        let sites = apache(lines);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].domains, vec!["blog.example.com", "www.blog.example.com"]);
        assert_eq!(sites[0].root.as_deref(), Some("/var/www/blog"));
    }
}
