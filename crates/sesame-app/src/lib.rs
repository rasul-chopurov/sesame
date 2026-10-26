//! The `sesame` app: GPUI window plus a CLI over the same engine.

rust_i18n::i18n!("locales", fallback = "en");

pub mod assets;
pub mod cli;
pub mod format;
pub mod i18n;
pub mod mcp;
pub mod rest;
pub mod runtime;
pub mod ui;
