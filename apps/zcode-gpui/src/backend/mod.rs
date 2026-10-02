//! Agent backend: process launch per workspace, the V4 NDJSON wire codec,
//! inbound event routing / response correlation and outbound commands.

pub(crate) mod commands;
pub(crate) mod events;
pub(crate) mod launcher;
pub(crate) mod responses;
pub(crate) mod session_cmds;
pub(crate) mod wire;
pub(crate) mod workspace;
