//! Library surface so integration tests can call into our modules
//! without going through the `main` binary entry point.

pub mod app;
pub mod config;
pub mod dcm;
pub mod logging;
pub mod ui;
