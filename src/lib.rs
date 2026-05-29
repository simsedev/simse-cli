//! simse-cli — simse terminal interface.
//!
//! All modules are gated on the `cli` feature.

pub mod app;
pub mod at_mention;
pub mod auth;
pub mod auth_cmd;
pub mod autocomplete;
pub mod banner;
pub mod cli_args;
pub mod commands;
pub mod config;
pub mod constants;
pub mod dispatch;
pub mod error_box;
pub mod event_loop;
pub mod handlers;
pub mod headless;
pub mod inference_client;
pub mod json_io;
pub mod levenshtein;
pub mod markdown;
pub mod marketplace;
pub mod memory_client;
pub mod openai_compat;
pub mod output;
pub mod plugin_loader;
pub mod proto;
pub mod protocol;
pub mod remote_cmd;
pub mod remote_transport;
pub mod search;
pub mod session_store;
pub mod spinner;
pub mod status_bar;
pub mod tool_call_box;
pub mod tools;
pub mod ui_core;
pub mod update;
