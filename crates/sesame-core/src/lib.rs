//! Sesame core: the data model, local storage, secrets and the operation
//! registry. No UI and no network here, so desktop, mobile, CLI, MCP and a
//! future SaaS backend all share it.

pub mod model;
pub mod ops;
pub mod secrets;
pub mod snapshot;
pub mod store;

pub use model::{AuthMethod, ServerConfig, ServerId};
pub use snapshot::ServerSnapshot;
pub use store::{AuditEntry, SqliteStore, Store};
