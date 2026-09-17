//! Loopback HTTP client for a running CG-Agent-Harness. No agentic, no writes.

pub mod client;
pub mod csrf;
pub mod discover;
pub mod display;
pub mod home;
mod http;
pub mod mood;
pub mod ollama;
pub mod origin;
pub mod paths;
pub mod theme;
pub mod validate;

#[cfg(target_os = "macos")]
pub mod app;

pub use client::{ChatReply, Client, Status};
pub use mood::Mood;
pub use origin::LoopbackOrigin;
