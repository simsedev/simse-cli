//! CLI-local tool implementations.
//!
//! These were re-homed from `core/src/tools` in the purify cut (core commit
//! 2cc29b3): the cloud-backed memory tools and the CLI sub-agent / ACP
//! delegate runners are CLI-only and no longer ship in the shared library.
//! They build on the shared tool-registry substrate from `simse_core::tools`.

pub mod memory;
pub mod subagent_cli;
