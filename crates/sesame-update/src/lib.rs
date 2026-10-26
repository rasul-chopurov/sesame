//! In-app updates from GitHub Releases.
//!
//! Each release carries per-platform archives and a detached ed25519
//! signature (`<archive>.sig`, base64). The public key is compiled in, so a
//! compromised download host cannot ship a build the app will install.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use semver::Version;
use serde::{Deserialize, Serialize};

pub const REPO: &str = "rasul-chopurov/sesame";

/// Release signing public key (ed25519, base64). Pair of the private key in
/// the `SESAME_SIGNING_KEY` GitHub secret.
pub const PUBLIC_KEY: &str = include_str!("../signing-key.pub");

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Only final releases.
    Stable,
    /// Also alpha/beta pre-releases.
    Beta,
}

impl Channel {
    /// Pre-release builds follow pre-releases by default.
    pub fn default_for(current: &Version) -> Self {
        if current.pre.is_empty() { Channel::Stable } else { Channel::Beta }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stable" => Some(Channel::Stable),
            "beta" => Some(Channel::Beta),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Release {
    pub version: Version,
    pub title: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    /// Archive for this platform and its signature, when the release has them.
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub signature_url: String,
}

/// The newest release worth offering plus the notes of every release since
/// the running one, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateInfo {
    pub latest: Release,
    pub changelog: Vec<Release>,
}

/// Archive name for this platform, e.g. `Sesame-0.1.0-macos-arm64.zip`.
pub fn asset_name(version: &Version) -> Option<String> {
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("windows", "x86_64") => "windows-x64",
        _ => return None,
    };
    Some(format!("Sesame-{version}-{platform}.zip"))
}

fn parse_releases(json: &str) -> Result<Vec<Release>> {
    let raw: Vec<GhRelease> = serde_json::from_str(json)?;
    let mut out = Vec::new();
    for r in raw.into_iter().filter(|r| !r.draft) {
        let Ok(version) = Version::parse(r.tag_name.trim_start_matches('v')) else { continue };
        let asset = asset_name(&version).and_then(|name| {
            let archive = r.assets.iter().find(|a| a.name == name)?;
            let sig = r.assets.iter().find(|a| a.name == format!("{name}.sig"))?;
            Some(Asset {
                name,
                url: archive.browser_download_url.clone(),
                signature_url: sig.browser_download_url.clone(),
            })
        });
        out.push(Release {
            title: r.name.filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Sesame {version}")),
            notes: r.body.unwrap_or_default(),
            published_at: r.published_at,
            prerelease: r.prerelease || !version.pre.is_empty(),
            version,
            asset,
        });
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(out)
}

/// Picks the update to offer from `releases` (any order).
pub fn select(releases: &[Release], current: &Version, channel: Channel) -> Option<UpdateInfo> {
    let mut newer: Vec<Release> = releases
        .iter()
        .filter(|r| r.version > *current)
        .filter(|r| channel == Channel::Beta || !r.prerelease)
        .cloned()
        .collect();
    newer.sort_by(|a, b| b.version.cmp(&a.version));
    let latest = newer.first()?.clone();
    Some(UpdateInfo { latest, changelog: newer })
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("sesame/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Asks GitHub for releases and returns the update to offer, if any.
pub async fn check(channel: Channel) -> Result<Option<UpdateInfo>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=50");
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let json = resp.error_for_status()?.text().await?;
    Ok(select(&parse_releases(&json)?, &current_version(), channel))
}

/// Downloads the archive for `release`, verifies its signature and returns
/// the path of the verified file (inside `dir`).
pub async fn download(release: &Release, dir: &Path) -> Result<PathBuf> {
    let asset = release
        .asset
        .as_ref()
        .context("this release has no build for your platform")?;
    let client = client()?;
    let bytes = client.get(&asset.url).send().await?.error_for_status()?.bytes().await?;
    let sig = client.get(&asset.signature_url).send().await?.error_for_status()?.text().await?;
    verify(&bytes, &sig, PUBLIC_KEY)?;
    let path = dir.join(&asset.name);
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

/// Checks a base64 ed25519 signature over `data` against a base64 public key.
pub fn verify(data: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let key: [u8; 32] = b64
        .decode(public_key_b64.trim())?
        .try_into()
        .map_err(|_| anyhow!("bad public key length"))?;
    let sig: [u8; 64] = b64
        .decode(signature_b64.trim())?
        .try_into()
        .map_err(|_| anyhow!("bad signature length"))?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key)?;
    key.verify_strict(data, &ed25519_dalek::Signature::from_bytes(&sig))
        .map_err(|_| anyhow!("signature does not match — refusing to install"))
}

/// Signs `data` with a base64 ed25519 secret key (used by `sesame-sign` in CI).
pub fn sign(data: &[u8], secret_key_b64: &str) -> Result<String> {
    use ed25519_dalek::Signer as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret: [u8; 32] = b64
        .decode(secret_key_b64.trim())?
        .try_into()
        .map_err(|_| anyhow!("bad secret key length"))?;
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    Ok(b64.encode(key.sign(data).to_bytes()))
}

/// New random keypair as (secret, public), base64.
pub fn keygen() -> Result<(String, String)> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|e| anyhow!("{e}"))?;
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    let b64 = base64::engine::general_purpose::STANDARD;
    Ok((b64.encode(secret), b64.encode(key.verifying_key().to_bytes())))
}

/// Where the running app is installed: the `.app` bundle on macOS, the exe
/// on Windows. `None` for development builds (`cargo run`).
pub fn installed_location() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(target_os = "macos") {
        // …/Sesame.app/Contents/MacOS/sesame
        let bundle = exe.parent()?.parent()?.parent()?;
        (bundle.extension()? == "app").then(|| bundle.to_path_buf())
    } else if cfg!(windows) {
        (!exe.components().any(|c| c.as_os_str() == "target")).then_some(exe)
    } else {
        None
    }
}

/// Replaces the installed app with the verified archive. The caller relaunches.
pub fn install(archive: &Path) -> Result<PathBuf> {
    let target = installed_location()
        .context("this is a development build; updates install only into a packaged app")?;
    install_into(archive, &target)?;
    Ok(target)
}

#[cfg(target_os = "macos")]
fn install_into(archive: &Path, bundle: &Path) -> Result<()> {
    let parent = bundle.parent().context("bundle has no parent")?;
    let staging = tempfile::Builder::new().prefix(".sesame-update").tempdir_in(parent)?;
    let status = std::process::Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(archive)
        .arg(staging.path())
        .status()?;
    if !status.success() {
        bail!("could not unpack the update");
    }
    let new_bundle = std::fs::read_dir(staging.path())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .context("archive contains no .app")?;
    let old = parent.join(".Sesame.app.old");
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(bundle, &old)?;
    if let Err(e) = std::fs::rename(&new_bundle, bundle) {
        std::fs::rename(&old, bundle)?; // put the working app back
        return Err(e.into());
    }
    let _ = std::fs::remove_dir_all(&old);
    // Downloaded via our own client, so no quarantine flag; clear it anyway.
    let _ = std::process::Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(bundle)
        .status();
    Ok(())
}

#[cfg(windows)]
fn install_into(archive: &Path, _exe: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut entry = zip.by_name("sesame.exe").context("archive contains no sesame.exe")?;
    let tmp = tempfile::NamedTempFile::new()?;
    std::io::copy(&mut entry, &mut tmp.as_file())?;
    self_replace::self_replace(tmp.path())?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn install_into(_archive: &Path, _target: &Path) -> Result<()> {
    bail!("in-app updates are not supported on this platform yet")
}

/// Starts the freshly installed app; the caller then quits.
pub fn relaunch(target: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/bin/open").arg("-n").arg(target).spawn()?;
    } else {
        std::process::Command::new(target).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(v: &str, pre: bool) -> Release {
        Release {
            version: Version::parse(v).unwrap(),
            title: v.into(),
            notes: format!("notes {v}"),
            published_at: None,
            prerelease: pre,
            asset: None,
        }
    }

    #[test]
    fn semver_orders_prereleases_before_final() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.1.0-alpha.2") > v("0.1.0-alpha.1"));
        assert!(v("0.1.0-alpha.10") > v("0.1.0-alpha.9"));
        assert!(v("0.1.0-beta.1") > v("0.1.0-alpha.10"));
        assert!(v("0.1.0") > v("0.1.0-beta.3"));
    }

    #[test]
    fn selects_by_channel_with_changelog() {
        let all = vec![
            release("0.1.0-alpha.1", true),
            release("0.1.0-alpha.2", true),
            release("0.1.0", false),
            release("0.2.0-beta.1", true),
        ];
        let cur = Version::parse("0.1.0-alpha.1").unwrap();

        let beta = select(&all, &cur, Channel::Beta).unwrap();
        assert_eq!(beta.latest.version.to_string(), "0.2.0-beta.1");
        let versions: Vec<String> = beta.changelog.iter().map(|r| r.version.to_string()).collect();
        assert_eq!(versions, ["0.2.0-beta.1", "0.1.0", "0.1.0-alpha.2"]);

        let stable = select(&all, &cur, Channel::Stable).unwrap();
        assert_eq!(stable.latest.version.to_string(), "0.1.0");
        assert_eq!(stable.changelog.len(), 1);

        assert!(select(&all, &Version::parse("0.2.0-beta.1").unwrap(), Channel::Beta).is_none());
    }

    #[test]
    fn parses_github_releases_and_finds_platform_asset() {
        let name = asset_name(&Version::parse("0.1.0-alpha.2").unwrap()).unwrap_or_default();
        let json = format!(
            r#"[
              {{"tag_name":"v0.1.0-alpha.2","name":"","body":"- New","draft":false,"prerelease":true,
                "assets":[{{"name":"{name}","browser_download_url":"https://x/a.zip"}},
                          {{"name":"{name}.sig","browser_download_url":"https://x/a.zip.sig"}}]}},
              {{"tag_name":"v9.9.9","draft":true,"prerelease":false}},
              {{"tag_name":"nightly","draft":false,"prerelease":true}}
            ]"#
        );
        let rs = parse_releases(&json).unwrap();
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].title, "Sesame 0.1.0-alpha.2");
        assert!(rs[0].prerelease);
        if !name.is_empty() {
            assert_eq!(rs[0].asset.as_ref().unwrap().signature_url, "https://x/a.zip.sig");
        }
    }

    #[test]
    fn signatures_round_trip_and_reject_tampering() {
        let (secret, public) = keygen().unwrap();
        let sig = sign(b"archive bytes", &secret).unwrap();
        verify(b"archive bytes", &sig, &public).unwrap();
        assert!(verify(b"archive bytez", &sig, &public).is_err());
        let (_, other) = keygen().unwrap();
        assert!(verify(b"archive bytes", &sig, &other).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn replaces_app_bundle_from_zip() {
        let root = tempfile::tempdir().unwrap();
        let make_bundle = |dir: &Path, marker: &str| {
            let macos = dir.join("Sesame.app/Contents/MacOS");
            std::fs::create_dir_all(&macos).unwrap();
            std::fs::write(macos.join("sesame"), marker).unwrap();
        };
        // Installed app.
        let apps = root.path().join("Applications");
        make_bundle(&apps, "old");
        // New build, zipped the way the release does it.
        let build = root.path().join("build");
        make_bundle(&build, "new");
        let zip = root.path().join("Sesame.zip");
        let ok = std::process::Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(build.join("Sesame.app"))
            .arg(&zip)
            .status()
            .unwrap();
        assert!(ok.success());

        install_into(&zip, &apps.join("Sesame.app")).unwrap();
        let exe = std::fs::read_to_string(apps.join("Sesame.app/Contents/MacOS/sesame")).unwrap();
        assert_eq!(exe, "new");
        let leftovers: Vec<_> = std::fs::read_dir(&apps).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("Sesame.app")]);

        // A broken archive leaves the installed app untouched.
        std::fs::write(&zip, b"not a zip").unwrap();
        assert!(install_into(&zip, &apps.join("Sesame.app")).is_err());
        let exe = std::fs::read_to_string(apps.join("Sesame.app/Contents/MacOS/sesame")).unwrap();
        assert_eq!(exe, "new");
    }

    #[test]
    fn embedded_public_key_is_valid() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let key: [u8; 32] = b64.decode(PUBLIC_KEY.trim()).unwrap().try_into().unwrap();
        ed25519_dalek::VerifyingKey::from_bytes(&key).unwrap();
    }
}
