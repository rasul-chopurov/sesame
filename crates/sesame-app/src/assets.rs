//! Embedded assets: the component icon set, the Lucide icons Sesame uses,
//! brand logos for detected software and the Sesame logo.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

gpui_kit::assets::icon_assets!(
    SesameIcons,
    [
        Activity, Boxes, CircleAlert, Clock, Cloud, Construction, Database, DoorOpen, Download,
        ExternalLink, Gauge, HardDrive, KeyRound, Languages, Layers, ListChecks, Lock, Mail,
        MemoryStick, Monitor, Package, Power, PowerOff, RefreshCw, Rocket, Server, Shield,
        Sparkles, Terminal, TriangleAlert, Wrench, X, Zap
    ]
);

macro_rules! embedded {
    ($($path:literal),* $(,)?) => {
        const EMBEDDED: &[(&str, &[u8])] = &[
            $(($path, include_bytes!(concat!("../assets/", $path)))),*
        ];
    };
}

embedded!(
    "logo.svg",
    "brands/apache.svg",
    "brands/caddy.svg",
    "brands/cloudflare.svg",
    "brands/debian.svg",
    "brands/docker.svg",
    "brands/elasticsearch.svg",
    "brands/github.svg",
    "brands/laravel.svg",
    "brands/mariadb.svg",
    "brands/minio.svg",
    "brands/mongodb.svg",
    "brands/mysql.svg",
    "brands/nginx.svg",
    "brands/nodedotjs.svg",
    "brands/php.svg",
    "brands/postgresql.svg",
    "brands/redis.svg",
    "brands/ubuntu.svg",
    "brands/wordpress.svg",
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = EMBEDDED.iter().find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some(bytes) = SesameIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out = gpui_kit::assets::Assets.list(path)?;
        out.extend(SesameIcons.list(path)?);
        out.extend(EMBEDDED.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| SharedString::from(*p)));
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// Brand logo path and colour for a piece of software, by simple-icons slug.
pub fn brand(slug: &str) -> Option<(&'static str, u32)> {
    Some(match slug {
        "laravel" => ("brands/laravel.svg", 0xFF2D20),
        "wordpress" => ("brands/wordpress.svg", 0x21759B),
        "nodedotjs" => ("brands/nodedotjs.svg", 0x5FA04E),
        "php" => ("brands/php.svg", 0x777BB4),
        "caddy" => ("brands/caddy.svg", 0x1F88C0),
        "nginx" => ("brands/nginx.svg", 0x009639),
        "apache" => ("brands/apache.svg", 0xD22128),
        "mysql" => ("brands/mysql.svg", 0x4479A1),
        "mariadb" => ("brands/mariadb.svg", 0xC0765A),
        "postgresql" => ("brands/postgresql.svg", 0x4169E1),
        "redis" => ("brands/redis.svg", 0xFF4438),
        "mongodb" => ("brands/mongodb.svg", 0x47A248),
        "elasticsearch" => ("brands/elasticsearch.svg", 0x00BFB3),
        "minio" => ("brands/minio.svg", 0xC72E49),
        "docker" => ("brands/docker.svg", 0x2496ED),
        "github" => ("brands/github.svg", 0x8B949E),
        "cloudflare" => ("brands/cloudflare.svg", 0xF38020),
        "ubuntu" => ("brands/ubuntu.svg", 0xE95420),
        "debian" => ("brands/debian.svg", 0xA81D33),
        _ => return None,
    })
}
