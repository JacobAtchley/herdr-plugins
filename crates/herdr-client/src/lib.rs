//! Typed client for the herdr socket API, shared by the plugins in this repo.

pub mod client;
pub mod context;
pub mod models;

pub use client::{Api, Client, Error};
pub use context::PluginContext;
