//! Resend: domains, verification and per-domain sending keys, plus the DNS
//! records Resend asks for as Cloudflare records.
//!
//! Needs a Resend API key with full access (sending-only keys cannot manage
//! domains). Sites get their own sending-only key limited to their domain.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sesame_cloudflare::DnsRecord;

const API: &str = "https://api.resend.com";

/// SMTP relay of Resend; the username is always `resend`, the password a key.
pub const SMTP_HOST: &str = "smtp.resend.com";
pub const SMTP_PORT: u16 = 587;
pub const SMTP_USER: &str = "resend";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Domain {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub records: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// What it is for: DKIM, SPF, MX, Tracking…
    #[serde(default)]
    pub record: String,
    /// Host relative to the domain (`send`, `resend._domainkey`), or `@`.
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub priority: Option<u16>,
    #[serde(default)]
    pub status: String,
}

pub struct Client {
    token: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct List<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    name: String,
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

    async fn call<T: serde::de::DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req.bearer_auth(&self.token).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            let msg = serde_json::from_str::<ApiError>(&body)
                .map(|e| format!("{} ({})", e.message, e.name))
                .unwrap_or_else(|_| body.chars().take(200).collect());
            bail!("Resend {status}: {msg}");
        }
        serde_json::from_str(&body).with_context(|| format!("unexpected Resend reply: {}", body.chars().take(200).collect::<String>()))
    }

    pub async fn domains(&self) -> Result<Vec<Domain>> {
        Ok(self.call::<List<Domain>>(self.http.get(format!("{API}/domains"))).await?.data)
    }

    pub async fn domain(&self, id: &str) -> Result<Domain> {
        self.call(self.http.get(format!("{API}/domains/{id}"))).await
    }

    pub async fn create_domain(&self, name: &str, region: &str) -> Result<Domain> {
        self.call(self.http.post(format!("{API}/domains")).json(&serde_json::json!({ "name": name, "region": region }))).await
    }

    /// Asks Resend to check the DNS records again.
    pub async fn verify(&self, id: &str) -> Result<()> {
        let _: serde_json::Value = self.call(self.http.post(format!("{API}/domains/{id}/verify"))).await?;
        Ok(())
    }

    pub async fn delete_domain(&self, id: &str) -> Result<()> {
        let _: serde_json::Value = self.call(self.http.delete(format!("{API}/domains/{id}"))).await?;
        Ok(())
    }

    /// A sending-only key limited to one domain; returns the token (shown once).
    pub async fn sending_key(&self, name: &str, domain_id: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Key {
            token: String,
        }
        let k: Key = self
            .call(self.http.post(format!("{API}/api-keys")).json(&serde_json::json!({
                "name": name, "permission": "sending_access", "domain_id": domain_id
            })))
            .await?;
        Ok(k.token)
    }

    /// Checks the key can list domains (i.e. it has full access).
    pub async fn check(&self) -> Result<()> {
        self.domains().await.map(|_| ())
    }
}

/// Full DNS name of a record Resend gives relative to `domain`.
pub fn full_name(name: &str, domain: &str) -> String {
    let name = name.trim_end_matches('.');
    if name.is_empty() || name == "@" || name == domain {
        domain.to_string()
    } else if name.ends_with(&format!(".{domain}")) {
        name.to_string()
    } else {
        format!("{name}.{domain}")
    }
}

/// The records of a Resend domain as Cloudflare records (DNS only, never proxied).
pub fn cloudflare_records(d: &Domain) -> Vec<DnsRecord> {
    d.records
        .iter()
        .map(|r| DnsRecord {
            id: String::new(),
            kind: r.kind.to_uppercase(),
            name: full_name(&r.name, &d.name),
            content: r.value.clone(),
            proxied: false,
            ttl: 1,
            priority: r.priority,
        })
        .collect()
}

/// How a desired record relates to what the zone already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Create(DnsRecord),
    /// Same name and type with another value; only TXT/MX/CNAME we own are updated.
    Update(DnsRecord),
    Same,
}

/// Plans the zone changes for `wanted`. An SPF TXT on the same name is
/// replaced only when it is ours (contains `amazonses.com`), so another
/// provider's SPF is never overwritten.
pub fn diff(wanted: &[DnsRecord], existing: &[DnsRecord]) -> Vec<Change> {
    wanted
        .iter()
        .map(|w| {
            let same_kind: Vec<&DnsRecord> = existing.iter().filter(|e| e.name == w.name && e.kind == w.kind).collect();
            if same_kind.iter().any(|e| e.content.trim_matches('"') == w.content) {
                return Change::Same;
            }
            let ours = same_kind.iter().find(|e| match w.kind.as_str() {
                "TXT" if w.content.starts_with("v=spf1") => e.content.contains("v=spf1") && e.content.contains("amazonses.com"),
                "TXT" => w.name.contains("_domainkey"),
                "MX" | "CNAME" => true,
                _ => false,
            });
            match ours {
                Some(e) => Change::Update(DnsRecord { id: e.id.clone(), ..w.clone() }),
                None => Change::Create(w.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain() -> Domain {
        serde_json::from_str(
            r#"{"id":"d1","name":"shop.example.com","status":"not_started","region":"eu-west-1","records":[
            {"record":"DKIM","name":"resend._domainkey","value":"p=MIGf","type":"TXT","status":"not_started","ttl":"Auto"},
            {"record":"SPF","name":"send","type":"MX","ttl":"Auto","status":"not_started","value":"feedback-smtp.eu-west-1.amazonses.com","priority":10},
            {"record":"SPF","name":"send","value":"v=spf1 include:amazonses.com ~all","type":"TXT","ttl":"Auto","status":"not_started"}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn records_become_full_names() {
        let rs = cloudflare_records(&domain());
        assert_eq!(rs[0].name, "resend._domainkey.shop.example.com");
        assert_eq!(rs[1].name, "send.shop.example.com");
        assert_eq!(rs[1].priority, Some(10));
        assert!(rs.iter().all(|r| !r.proxied));
        assert_eq!(full_name("@", "a.com"), "a.com");
        assert_eq!(full_name("send.a.com", "a.com"), "send.a.com");
    }

    #[test]
    fn diff_keeps_foreign_spf() {
        let wanted = cloudflare_records(&domain());
        let foreign = DnsRecord { id: "x".into(), kind: "TXT".into(), name: "send.shop.example.com".into(), content: "v=spf1 include:_spf.google.com ~all".into(), proxied: false, ttl: 1, priority: None };
        let same = DnsRecord { id: "y".into(), content: "\"p=MIGf\"".into(), name: "resend._domainkey.shop.example.com".into(), ..foreign.clone() };
        let d = diff(&wanted, &[foreign, same]);
        assert_eq!(d[0], Change::Same);
        assert!(matches!(d[1], Change::Create(_)));
        assert!(matches!(&d[2], Change::Create(r) if r.content.contains("amazonses")), "foreign SPF is not overwritten");
    }
}
