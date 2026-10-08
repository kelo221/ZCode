//! Native-only Services stdio transport acceptance contract.
//!
//! The unchanged public `packages/rpc` serialization, SocketProtocol and
//! ChannelServer contracts are authoritative; this is not CLI NDJSON or
//! PersistentProtocol. Application integration, isolation and method allowlists
//! belong to the caller. This module never discovers storage or replays calls.
//!
//! ```text
//! background spawn -> bounded hello -> hello-ack -> Initialize(undefined)
//! call -> Shared admission lock -> bounded FIFO -> writer -> existing Host
//! Host reply -> reader -> same Shared pending owner -> one-shot result
//! cancel -> same admission lock -> FIFO cancel -> local cancelled result
//! EOF/write failure/drop -> Shared terminal transition -> all pending settled
//!                        -> owned child tree killed; reaper waits off-thread
//! ```
//!
//! IDs are monotonic within one owned connection, never reused or transferred.
//! Only one lock owns accepted calls, queued bytes and terminal state. Cancellation
//! is advisory, not mutation rollback; dropping a call does not cancel a mutation.
//! `ServiceValue::Undefined` is successful void, distinct from JSON null. Replies
//! to already settled/cancelled IDs are ignored; future IDs and malformed messages
//! fail closed. There are no subscriptions, reverse calls or mobile replay semantics.
//! Public results support JSON/void only; direct binary tags are parsed internally,
//! nested Uint8Array markers fail explicitly rather than silently changing TS types.
//! `ServiceCall::wait` races its receiver against the caller-owned async deadline;
//! deadline expiry sends advisory cancellation and reports uncertain mutation outcome.
//! A cancellation queue failure closes the transport, never leaves orphaned pending.
//! Dropping a wait future, like dropping a call, does not cancel accepted mutation.
//!
//! Bounds: 2 MiB frame, 1 MiB text/JSON, depth 32, 65,536 collection elements,
//! 100,000 wire nodes, 64 pending calls, 128 queued frames / 4 MiB queued bytes.
//! Hello is at most 8 KiB. Startup waits at most 10 s for hello and 30 s for
//! Initialize; those deadlines abort failed startup, never sequence normal IO.
//! IO threads retain no stderr content (fixed-size draining); errors contain no
//! remote payload, stack, stderr, caller paths or OS error strings. Shutdown is
//! idempotent and does not join IO threads on the caller/UI thread.
//!
//! Acceptance tests precede implementation: reference bytes, fragmented/coalesced
//! streams, signed VQL, undefined/null, bounded invalid/truncated input, hello,
//! initialization, correlation, errors, EOF, cancellation and process cleanup.
//! A test-only synthetic child has no Host/service/persistence implementation.
//! Real Host/isolation/UI acceptance remains owned by application integration.

mod client;
mod codec;
mod exit;
mod frame;
mod message;
mod process;
mod process_tree;
mod state;

pub use client::{ServiceCall, ServiceClient, ServiceExitObserver, ServiceSpawnError};
pub use message::ServiceValue;

#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod codec_tests;
#[cfg(test)]
pub(crate) mod fake_child;
#[cfg(test)]
mod process_tree_tests;
