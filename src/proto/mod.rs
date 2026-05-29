//! CLI-local generated protobuf types.
//!
//! After the core purify cut (commit 2cc29b3) core stopped shipping the
//! `foundry.adaptive` proto to library consumers, so the CLI compiles its
//! own copy via `build.rs` (from `../foundry/proto/foundry/adaptive.proto`).
//! These are the `AdaptiveService` memory message types the CLI dials over
//! gRPC-Web through `cloud/api` (see `crate::memory_client`).

#[allow(clippy::all, clippy::pedantic)]
pub mod adaptive {
    include!("foundry.adaptive.rs");
}
