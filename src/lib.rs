//! Small, general purpose file manager built using GTK.
//!
//! Generally, each top-level module corresponds to a different Relm4 component.

#![warn(clippy::dbg_macro)]
#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]
#![warn(clippy::todo)]

mod component;
mod config;

pub use component::app::{AppModel, AppMode};

pub const APP_ID: &str = "io.miguelbaldi.KRust";
