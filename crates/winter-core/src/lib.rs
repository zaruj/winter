//! Application logic for winter.
//!
//! This crate must not depend on Tauri: the GUI in `src-tauri` is a thin layer on top of it.

pub mod aws;
mod error;
pub mod github;
pub mod k8s;
pub mod state;
pub mod terminal;
pub mod terminal_profile;
pub mod tls;

pub use error::{Error, Result};
