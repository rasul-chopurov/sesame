//! Every action Sesame can take is described once here. The GUI, the REST API
//! and the MCP server all list and call the same operations, so a feature
//! added for the app is available to AI agents the same day.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How dangerous an operation is. Agents need confirmation in the app for
/// anything above `Read` unless their token allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Read,
    Change,
    Destructive,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: Risk,
}

/// Operations implemented so far. Input/output schemas are attached by the
/// crates that implement them (`sesame-discovery`, …) when the API layer
/// registers handlers.
pub const OPS: &[OpSpec] = &[
    OpSpec {
        name: "list_servers",
        description: "List the servers added to Sesame.",
        risk: Risk::Read,
    },
    OpSpec {
        name: "discover_server",
        description: "Connect over SSH and find everything on a server: OS, services, sites, databases, mail, PHP, cron, certificates, firewall. Read-only.",
        risk: Risk::Read,
    },
    OpSpec {
        name: "get_snapshot",
        description: "Return the last discovery result for a server without connecting.",
        risk: Risk::Read,
    },
];

pub fn find(name: &str) -> Option<&'static OpSpec> {
    OPS.iter().find(|op| op.name == name)
}
