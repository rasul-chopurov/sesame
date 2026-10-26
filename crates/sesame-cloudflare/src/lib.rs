//! Cloudflare API v4 client and the logic that ties sites to DNS.
//!
//! Needs an API token with `Zone:Read`, `DNS:Edit` and, to add domains,
//! `Zone:Edit` + `Account:Read`.

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const API: &str = "https://api.cloudflare.com/client/v4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub name_servers: Vec<String>,
    #[serde(default)]
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRecord {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub proxied: bool,
    #[serde(default = "auto_ttl")]
    pub ttl: u32,
    /// MX (and SRV) priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u16>,
}

/// Record types Sesame lets you add, in Cloudflare's order.
pub const RECORD_TYPES: &[&str] = &["A", "AAAA", "CNAME", "TXT", "MX", "NS", "CAA"];

/// Types Cloudflare can proxy.
pub fn proxiable(kind: &str) -> bool {
    matches!(kind, "A" | "AAAA" | "CNAME")
}

fn record_body(r: &DnsRecord) -> serde_json::Value {
    let mut body = serde_json::json!({ "type": r.kind, "name": r.name, "content": r.content, "ttl": r.ttl });
    if proxiable(&r.kind) {
        body["proxied"] = serde_json::json!(r.proxied);
    }
    if let Some(p) = r.priority {
        body["priority"] = serde_json::json!(p);
    }
    body
}

fn auto_ttl() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<ApiError>,
    result: Option<T>,
    #[serde(default)]
    result_info: Option<ResultInfo>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ResultInfo {
    #[serde(default)]
    total_pages: u32,
}

pub struct Client {
    token: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            token: token.into(),
            http: reqwest::Client::builder()
                .user_agent(concat!("sesame/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }

    async fn call<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<(T, u32)> {
        let resp = req.bearer_auth(&self.token).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        let env: Envelope<T> = serde_json::from_str(&body)
            .with_context(|| format!("Cloudflare returned {status}: {}", body.chars().take(200).collect::<String>()))?;
        if !env.success {
            let msg = env.errors.iter().map(|e| format!("{} ({})", e.message, e.code)).collect::<Vec<_>>().join("; ");
            bail!("Cloudflare: {msg}");
        }
        let pages = env.result_info.map(|i| i.total_pages).unwrap_or(1);
        Ok((env.result.context("Cloudflare returned no result")?, pages))
    }

    /// Checks the token is active.
    pub async fn verify(&self) -> Result<()> {
        #[derive(Deserialize)]
        struct V {
            status: String,
        }
        let (v, _): (V, _) = self.call(self.http.get(format!("{API}/user/tokens/verify"))).await?;
        if v.status != "active" {
            bail!("token is {}", v.status);
        }
        Ok(())
    }

    pub async fn accounts(&self) -> Result<Vec<Account>> {
        Ok(self.call(self.http.get(format!("{API}/accounts?per_page=50"))).await?.0)
    }

    pub async fn zones(&self) -> Result<Vec<Zone>> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let (mut zones, pages): (Vec<Zone>, _) =
                self.call(self.http.get(format!("{API}/zones?per_page=50&page={page}"))).await?;
            out.append(&mut zones);
            if page >= pages {
                break;
            }
            page += 1;
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Adds a domain to the account. The result carries the nameservers the
    /// user must set at their registrar.
    pub async fn create_zone(&self, account_id: &str, name: &str) -> Result<Zone> {
        let body = serde_json::json!({ "name": name, "account": { "id": account_id }, "type": "full" });
        Ok(self.call(self.http.post(format!("{API}/zones")).json(&body)).await?.0)
    }

    pub async fn records(&self, zone_id: &str) -> Result<Vec<DnsRecord>> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let (mut recs, pages): (Vec<DnsRecord>, _) = self
                .call(self.http.get(format!("{API}/zones/{zone_id}/dns_records?per_page=100&page={page}")))
                .await?;
            out.append(&mut recs);
            if page >= pages {
                break;
            }
            page += 1;
        }
        out.sort_by(|a, b| (a.name.as_str(), a.kind.as_str()).cmp(&(b.name.as_str(), b.kind.as_str())));
        Ok(out)
    }

    pub async fn create_record(&self, zone_id: &str, r: &DnsRecord) -> Result<DnsRecord> {
        let body = record_body(r);
        Ok(self.call(self.http.post(format!("{API}/zones/{zone_id}/dns_records")).json(&body)).await?.0)
    }

    pub async fn update_record(&self, zone_id: &str, r: &DnsRecord) -> Result<DnsRecord> {
        let body = record_body(r);
        Ok(self
            .call(self.http.put(format!("{API}/zones/{zone_id}/dns_records/{}", r.id)).json(&body))
            .await?
            .0)
    }

    pub async fn delete_record(&self, zone_id: &str, record_id: &str) -> Result<()> {
        let _: (serde_json::Value, _) =
            self.call(self.http.delete(format!("{API}/zones/{zone_id}/dns_records/{record_id}"))).await?;
        Ok(())
    }

    pub async fn purge_everything(&self, zone_id: &str) -> Result<()> {
        let body = serde_json::json!({ "purge_everything": true });
        let _: (serde_json::Value, _) =
            self.call(self.http.post(format!("{API}/zones/{zone_id}/purge_cache")).json(&body)).await?;
        Ok(())
    }
}

/// The zone a hostname belongs to (longest matching suffix).
pub fn zone_for<'a>(host: &str, zones: &'a [Zone]) -> Option<&'a Zone> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    zones
        .iter()
        .filter(|z| host == z.name || host.ends_with(&format!(".{}", z.name)))
        .max_by_key(|z| z.name.len())
}

/// How a site's hostname looks in Cloudflare relative to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DnsStatus {
    /// No zone in the account covers this hostname.
    NoZone,
    /// Zone exists but has no A/AAAA/CNAME for the hostname.
    Missing { zone: String },
    /// Points somewhere else.
    Elsewhere { zone: String, record: DnsRecord },
    /// Points at the server.
    Ok { zone: String, record: DnsRecord },
}

pub fn status(host: &str, server_ip: &str, zones: &[Zone], records: &[(String, Vec<DnsRecord>)]) -> DnsStatus {
    let Some(zone) = zone_for(host, zones) else { return DnsStatus::NoZone };
    let recs = records.iter().find(|(id, _)| *id == zone.id).map(|(_, r)| r.as_slice()).unwrap_or(&[]);
    let host = host.to_ascii_lowercase();
    let matching: Vec<&DnsRecord> = recs
        .iter()
        .filter(|r| r.name.eq_ignore_ascii_case(&host) && matches!(r.kind.as_str(), "A" | "AAAA" | "CNAME"))
        .collect();
    if let Some(r) = matching.iter().find(|r| r.content == server_ip) {
        return DnsStatus::Ok { zone: zone.name.clone(), record: (*r).clone() };
    }
    match matching.first() {
        Some(r) => DnsStatus::Elsewhere { zone: zone.name.clone(), record: (*r).clone() },
        None => DnsStatus::Missing { zone: zone.name.clone() },
    }
}

/// The record Sesame proposes to point a hostname at a server.
pub fn record_for(host: &str, server_ip: &str, proxied: bool) -> DnsRecord {
    let kind = if server_ip.contains(':') { "AAAA" } else { "A" };
    DnsRecord {
        id: String::new(),
        kind: kind.into(),
        name: host.to_ascii_lowercase(),
        content: server_ip.into(),
        proxied,
        ttl: 1,
        priority: None,
    }
}

/// Valid hostname a user may add as a zone or record name.
pub fn valid_hostname(s: &str) -> bool {
    let s = s.trim_end_matches('.');
    !s.is_empty()
        && s.len() <= 253
        && s.contains('.')
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '*')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(id: &str, name: &str) -> Zone {
        Zone { id: id.into(), name: name.into(), status: "active".into(), name_servers: vec![], paused: false }
    }

    #[test]
    fn parses_api_envelopes() {
        let body = r#"{"success":true,"errors":[],"result":[{"id":"z1","name":"example.com","status":"active","name_servers":["ada.ns.cloudflare.com","bob.ns.cloudflare.com"],"paused":false}],"result_info":{"page":1,"total_pages":1}}"#;
        let env: Envelope<Vec<Zone>> = serde_json::from_str(body).unwrap();
        assert!(env.success);
        assert_eq!(env.result.unwrap()[0].name_servers.len(), 2);
        let err = r#"{"success":false,"errors":[{"code":9109,"message":"Invalid access token"}],"result":null}"#;
        let env: Envelope<Vec<Zone>> = serde_json::from_str(err).unwrap();
        assert_eq!(env.errors[0].code, 9109);
    }

    #[test]
    fn matches_longest_zone_suffix() {
        let zones = vec![zone("1", "example.com"), zone("2", "eu.example.com"), zone("3", "other.org")];
        assert_eq!(zone_for("shop.eu.example.com", &zones).unwrap().id, "2");
        assert_eq!(zone_for("example.com", &zones).unwrap().id, "1");
        assert_eq!(zone_for("www.example.com.", &zones).unwrap().id, "1");
        assert!(zone_for("notexample.com", &zones).is_none());
    }

    #[test]
    fn dns_status_for_sites() {
        let zones = vec![zone("1", "example.com")];
        let a = |name: &str, ip: &str| DnsRecord {
            id: "r".into(),
            kind: "A".into(),
            name: name.into(),
            content: ip.into(),
            proxied: true,
            ttl: 1,
            priority: None,
        };
        let records = vec![("1".to_string(), vec![a("example.com", "1.2.3.4"), a("old.example.com", "9.9.9.9")])];
        assert!(matches!(status("example.com", "1.2.3.4", &zones, &records), DnsStatus::Ok { .. }));
        assert!(matches!(status("old.example.com", "1.2.3.4", &zones, &records), DnsStatus::Elsewhere { .. }));
        assert_eq!(status("new.example.com", "1.2.3.4", &zones, &records), DnsStatus::Missing { zone: "example.com".into() });
        assert_eq!(status("shop.test", "1.2.3.4", &zones, &records), DnsStatus::NoZone);
        assert_eq!(record_for("New.Example.com", "2a01::1", true).kind, "AAAA");
    }

    #[test]
    fn record_bodies() {
        let mx = DnsRecord { id: String::new(), kind: "MX".into(), name: "example.com".into(), content: "mx.zoho.eu".into(), proxied: true, ttl: 3600, priority: Some(10) };
        let b = record_body(&mx);
        assert_eq!(b["priority"], 10);
        assert!(b.get("proxied").is_none(), "MX cannot be proxied");
        let a = record_for("example.com", "1.2.3.4", true);
        assert_eq!(record_body(&a)["proxied"], true);
    }

    #[test]
    fn hostnames() {
        assert!(valid_hostname("example.com"));
        assert!(valid_hostname("*.example.com"));
        assert!(!valid_hostname("localhost"));
        assert!(!valid_hostname("-bad.com"));
        assert!(!valid_hostname("a b.com"));
    }
}
