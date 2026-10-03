//! Agent backend: process launch per workspace, the V4 NDJSON wire codec,
//! inbound event routing / response correlation and outbound commands.

pub(crate) mod commands;
pub(crate) mod conn;
pub(crate) mod events;
pub(crate) mod flow_cmds;
pub(crate) mod launcher;
pub(crate) mod plugin_cmds;
pub(crate) mod plugin_payloads;
pub(crate) mod query_cmds;
pub(crate) mod responses;
pub(crate) mod reverse_rpc;
pub(crate) mod session_cmds;
pub(crate) mod wire;
pub(crate) mod workflow_cmds;
pub(crate) mod workspace;
