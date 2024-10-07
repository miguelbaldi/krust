#![warn(clippy::dbg_macro)]
// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]
#![warn(clippy::todo)]

mod backend;
mod component;
pub mod config;
mod modals;

pub use backend::repository::Repository;
pub use backend::settings::Settings;
pub use component::app::AppModel;
pub use component::app::AppMsg;
pub use component::app::TOASTER_BROKER;
pub use component::messages::messages_tab::MessagesSearchAction;

pub const KRUST_QUALIFIER: &str = "io";
pub const KRUST_ORGANIZATION: &str = "miguelbaldi";
pub const KRUST_APPLICATION: &str = "KRust";
pub const APP_ID: &str = "io.miguelbaldi.KRust";
pub const APP_RESOURCE_PATH: &str = "/io/miguelbaldi/KRust/";
pub const APP_NAME: &str = "KRust Kafka Client";
pub const VERSION: &str = env!("VERGEN_GIT_DESCRIBE");
pub const DATE_TIME_FORMAT: &str = "%d/%m/%Y %H:%M:%S";
pub const DATE_TIME_WITH_MILLIS_FORMAT: &str = "%d/%m/%Y %H:%M:%S%.3f";
pub const LOGO_SIZE: i32 = 800;
