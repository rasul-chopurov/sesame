//! Release signing helper.
//!   sesame-sign keygen            → prints secret and public key (base64)
//!   sesame-sign sign <file>…      → writes <file>.sig using $SESAME_SIGNING_KEY

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let (secret, public) = sesame_update::keygen()?;
            println!("secret: {secret}\npublic: {public}");
        }
        Some("sign") if args.len() > 1 => {
            let key = std::env::var("SESAME_SIGNING_KEY").context("SESAME_SIGNING_KEY is not set")?;
            for file in &args[1..] {
                let data = std::fs::read(file).with_context(|| format!("read {file}"))?;
                std::fs::write(format!("{file}.sig"), sesame_update::sign(&data, &key)?)?;
                println!("signed {file}");
            }
        }
        _ => bail!("usage: sesame-sign keygen | sesame-sign sign <file>…"),
    }
    Ok(())
}
