//! `sesame` — the desktop app, and a CLI for scripts and AI agents.

use clap::Parser;
use sesame_app::{cli, runtime, ui};

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    match args.command {
        None => {
            ui::run();
            Ok(())
        }
        Some(cmd) => runtime::get().block_on(cli::run(cmd)),
    }
}
