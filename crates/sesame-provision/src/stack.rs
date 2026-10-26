//! Prepare a server for Laravel from zero — or complete one that is half
//! set up. Every step checks first and only installs what is missing, so it
//! never replaces a PHP version or web server that sites already use.

use anyhow::{Result, bail};
use sesame_core::ops::Risk;
use sesame_core::snapshot::{ServerSnapshot, WebServerKind};
use sesame_ops::{Plan, Step};

use crate::{APT, preflight};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Web {
    Caddy,
    Nginx,
}

#[derive(Debug, Clone)]
pub struct StackOptions {
    pub web: Web,
    /// PHP for a server without PHP, e.g. `8.4`. Ignored when PHP exists.
    pub php: String,
    /// ufw allowing only SSH, HTTP and HTTPS.
    pub firewall: bool,
}

impl Default for StackOptions {
    fn default() -> Self {
        Self { web: Web::Caddy, php: "8.4".into(), firewall: true }
    }
}

/// What a snapshot is missing for a Laravel server.
pub fn missing(s: &ServerSnapshot) -> Vec<&'static str> {
    let mut m = Vec::new();
    if s.web_servers.is_empty() {
        m.push("web server");
    }
    if s.php.is_empty() {
        m.push("PHP");
    }
    if !s.services.iter().any(|u| u.unit.starts_with("supervisor")) {
        m.push("supervisor");
    }
    if s.firewall.ufw.as_deref() != Some("active") {
        m.push("firewall");
    }
    m
}

const EXTENSIONS: &str = "cli fpm common mbstring xml curl zip bcmath intl gd mysql pgsql sqlite3 redis opcache readline";

pub fn prepare_server(o: &StackOptions, s: &ServerSnapshot) -> Result<Plan> {
    if !o.php.chars().all(|c| c.is_ascii_digit() || c == '.') || o.php.is_empty() {
        bail!("invalid PHP version");
    }
    let has_caddy = s.web_servers.iter().any(|w| matches!(w.kind, WebServerKind::Caddy | WebServerKind::FrankenPhp));
    let has_nginx = s.web_servers.iter().any(|w| w.kind == WebServerKind::Nginx);
    let mut steps = vec![Step::new(
        "base packages and security updates",
        format!(
            "{}\n{APT} update >/dev/null\n\
             {APT} -o Dpkg::Options::=--force-confold upgrade >/dev/null\n\
             {APT} install curl ca-certificates gnupg git unzip acl supervisor unattended-upgrades fail2ban ufw lsb-release >/dev/null\n\
             echo 'unattended-upgrades unattended-upgrades/enable_auto_updates boolean true' | debconf-set-selections\n\
             dpkg-reconfigure -f noninteractive unattended-upgrades >/dev/null 2>&1 || true\n\
             systemctl enable --now supervisor fail2ban >/dev/null 2>&1\n\
             echo 'base ready'",
            preflight()
        ),
    )];
    steps.push(Step::new(
        "swap for small servers",
        "if [ -n \"$(swapon --show --noheadings)\" ]; then echo 'swap already on'; exit 0; fi\n\
         mem=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo)\n\
         if [ \"$mem\" -ge 8000 ]; then echo \"${mem} MB RAM, no swap needed\"; exit 0; fi\n\
         fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile >/dev/null && swapon /swapfile\n\
         grep -q '^/swapfile ' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab\n\
         sysctl -q vm.swappiness=10 && echo 'vm.swappiness=10' > /etc/sysctl.d/90-sesame-swap.conf\n\
         echo '2 GB swap on'",
    ));
    let php = &o.php;
    steps.push(Step::new(
        "PHP and the extensions Laravel needs",
        format!(
            "v=$(php -r 'echo PHP_MAJOR_VERSION.\".\".PHP_MINOR_VERSION;' 2>/dev/null || true)\n\
             if [ -z \"$v\" ]; then\n\
             \x20 v={php}\n\
             \x20 if ! apt-cache show php$v-fpm >/dev/null 2>&1; then\n\
             \x20   if [ \"$ID\" = ubuntu ]; then\n\
             \x20     {APT} install software-properties-common >/dev/null && add-apt-repository -y ppa:ondrej/php >/dev/null\n\
             \x20   else\n\
             \x20     curl -fsSL https://packages.sury.org/php/apt.gpg -o /usr/share/keyrings/sury-php.gpg\n\
             \x20     echo \"deb [signed-by=/usr/share/keyrings/sury-php.gpg] https://packages.sury.org/php/ $VERSION_CODENAME main\" > /etc/apt/sources.list.d/sury-php.list\n\
             \x20   fi\n\
             \x20   {APT} update >/dev/null\n\
             \x20 fi\n\
             else\n\
             \x20 echo \"PHP $v is already here; keeping it\"\n\
             fi\n\
             pkgs=''\n\
             for e in {EXTENSIONS}; do apt-cache show php$v-$e >/dev/null 2>&1 && pkgs=\"$pkgs php$v-$e\"; done\n\
             {APT} install $pkgs >/dev/null\n\
             systemctl enable --now php$v-fpm >/dev/null\n\
             php -v | head -1"
        ),
    ));
    steps.push(Step::new(
        "Composer (signature checked)",
        "if command -v composer >/dev/null; then composer --version 2>/dev/null | head -1; exit 0; fi\n\
         tmp=$(mktemp -d)\n\
         expected=$(curl -fsSL https://composer.github.io/installer.sig)\n\
         curl -fsSL https://getcomposer.org/installer -o \"$tmp/installer.php\"\n\
         actual=$(php -r \"echo hash_file('sha384', '$tmp/installer.php');\")\n\
         [ \"$expected\" = \"$actual\" ] || { echo 'Composer installer signature mismatch'; rm -rf \"$tmp\"; exit 1; }\n\
         php \"$tmp/installer.php\" --quiet --install-dir=/usr/local/bin --filename=composer\n\
         rm -rf \"$tmp\"\ncomposer --version | head -1",
    ));
    let web = match (o.web, has_caddy, has_nginx) {
        (_, true, _) | (_, _, true) => Step::new("web server", "echo 'a web server is already installed; keeping it'"),
        (Web::Caddy, ..) => Step::new(
            "Caddy (official repository)",
            format!(
                "curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key | gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg\n\
                 curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt -o /etc/apt/sources.list.d/caddy-stable.list\n\
                 {APT} update >/dev/null && {APT} install caddy >/dev/null\n\
                 install -d /etc/caddy/sites\n\
                 grep -q '^import /etc/caddy/sites/' /etc/caddy/Caddyfile 2>/dev/null || printf '\\nimport /etc/caddy/sites/*.caddy\\n' >> /etc/caddy/Caddyfile\n\
                 # The package's default site answers :80 with a placeholder page; Sesame sites bring their own.\n\
                 sed -i 's|^:80 {{|http://localhost {{|' /etc/caddy/Caddyfile\n\
                 caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null 2>&1 && systemctl reload caddy\n\
                 caddy version"
            ),
        ),
        (Web::Nginx, ..) => Step::new(
            "nginx",
            format!("{APT} install nginx >/dev/null\nrm -f /etc/nginx/sites-enabled/default\nnginx -t 2>/dev/null && systemctl reload nginx\nnginx -v 2>&1"),
        ),
    };
    steps.push(web);
    if o.firewall {
        steps.push(Step::new(
            "firewall: only SSH, HTTP and HTTPS",
            "if ufw status | grep -q 'Status: active'; then echo 'ufw already active; rules kept'; ufw status | sed -n '1,12p'; exit 0; fi\n\
             port=$(sshd -T 2>/dev/null | awk '$1==\"port\" {print $2; exit}')\n\
             [ -n \"$port\" ] || port=22\n\
             ufw default deny incoming >/dev/null && ufw default allow outgoing >/dev/null\n\
             ufw allow \"$port\"/tcp >/dev/null && ufw allow 80/tcp >/dev/null && ufw allow 443/tcp >/dev/null && ufw allow 443/udp >/dev/null\n\
             ufw --force enable >/dev/null\n\
             ufw status | sed -n '1,12p'",
        ));
    }
    let mut details = vec![
        "only what is missing is installed; existing PHP and web servers are kept".into(),
        "security updates install automatically (unattended-upgrades)".into(),
        format!("PHP {php} with Laravel's extensions and Composer, if there is no PHP yet"),
        "supervisor for queue workers, fail2ban for SSH".into(),
    ];
    if o.firewall {
        details.push("ufw: incoming only SSH (your current port), 80 and 443 — check the provider's firewall too".into());
    }
    Ok(Plan {
        op: "prepare_server",
        title: "prepare the server for Laravel".into(),
        risk: Risk::Change,
        details,
        confirm_word: None,
        steps,
        reveal: vec![],
        secret_scope: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_is_valid_shell_and_keeps_existing_web() {
        let p = prepare_server(&StackOptions::default(), &ServerSnapshot::default()).unwrap();
        for step in &p.steps {
            let ok = std::process::Command::new("sh").args(["-n", "-c", &step.script]).status().unwrap().success();
            assert!(ok, "{}:\n{}", step.title, step.script);
        }
        assert!(p.steps.iter().any(|s| s.title.contains("Caddy")));
        let mut snap = ServerSnapshot::default();
        snap.web_servers.push(sesame_core::snapshot::WebServer { kind: WebServerKind::Nginx, version: None, config_path: None });
        let p = prepare_server(&StackOptions::default(), &snap).unwrap();
        assert!(!p.steps.iter().any(|s| s.title.contains("Caddy")));
        assert!(prepare_server(&StackOptions { php: "8.4; rm".into(), ..Default::default() }, &snap).is_err());
    }
}
