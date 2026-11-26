// lib.rs — library target of the crate.
//
// Needed so integration tests in tests/ can import modules
// (e.g. `use ailimits::config::schema::Config`). A binary crate
// alone cannot be imported from tests.

pub mod app;
pub mod choices;
pub mod config;
pub mod consent;
pub mod hooks;
pub mod models;
pub mod monitor;
pub mod notifications;
pub mod platform;
pub mod providers;
pub mod rows;
pub mod secrets;
pub mod ui;
pub mod updater;
pub mod utc;
