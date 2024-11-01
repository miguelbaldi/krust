#![windows_subsystem = "windows"]
// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use std::env;

use gtk::gdk;
use gtk::gio;
use gtk::gio::ApplicationFlags;
use gtk::prelude::ApplicationExt;
use krust::Settings;
use krust::APP_RESOURCE_PATH;
use krust::TOASTER_BROKER;
use relm4::RELM_BLOCKING_THREADS;
use relm4::RELM_THREADS;
use tracing::*;
use tracing_subscriber::filter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_tree::HierarchicalLayer;

use relm4::{
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    gtk, RelmApp,
};

use krust::{AppModel, Repository, APP_ID};

relm4::new_action_group!(AppActionGroup, "app");
relm4::new_stateless_action!(QuitAction, AppActionGroup, "quit");

fn initialize_resources() {
    gio::resources_register_include!("resources.gresource").unwrap();
    gio::resources_register_include!("icons.gresource").unwrap();
    let display = gdk::Display::default().unwrap();
    let theme = gtk::IconTheme::for_display(&display);
    theme.add_resource_path("/org/miguelbaldi/krust/icons/");
}

fn main() -> Result<(), ()> {
    let threads_number = Settings::read().unwrap_or_default().threads_number as usize;
    RELM_THREADS.set(threads_number).unwrap();
    RELM_BLOCKING_THREADS.set(threads_number).unwrap();
    let filter = filter::Targets::new()
        // Enable the `INFO` level for anything in `my_crate`
        .with_target("relm4", Level::WARN)
        // Enable the `DEBUG` level for a specific module.
        .with_target("krust", Level::TRACE);
    tracing_subscriber::registry()
        .with(HierarchicalLayer::new(2))
        .with(EnvFilter::from_default_env())
        .with(filter)
        .init();

    info!("RELM_THREADS[{}]", threads_number);
    let gsk_renderer_var = "GSK_RENDERER";
    let render = match env::var(gsk_renderer_var) {
        Ok(render) => {
            info!("GSK_RENDERER[external]::{}", render);
            render
        }
        Err(_) => {
            let render = "gl";
            env::set_var(gsk_renderer_var, render);
            render.to_string()
        }
    };
    info!(
        "GSK_RENDERER[after]:: intended={}, actual={:?}",
        render,
        env::var(gsk_renderer_var)
    );
    // Call `gtk::init` manually because we instantiate GTK types in the app model.
    gtk::init().expect("should initialize GTK");
    if let Some(settings) = gtk::Settings::default() {
        info!(
            "prefer dark theme?: {}",
            settings.is_gtk_application_prefer_dark_theme()
        );
        //StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        //settings.set_gtk_application_prefer_dark_theme(true);
    };
    info!("starting application: {}", APP_ID);
    initialize_resources();
    gtk::Window::set_default_icon_name(APP_ID);
    let app = adw::Application::new(Some(APP_ID), ApplicationFlags::NON_UNIQUE);
    app.set_resource_base_path(Some(APP_RESOURCE_PATH));
    app.connect_startup(|_| {
        info!("initializing database");
        let mut repo = Repository::new();
        repo.init().expect("unable to initialize database");
    });

    let mut actions = RelmActionGroup::<AppActionGroup>::new();

    let quit_action = {
        let app = app.clone();
        RelmAction::<QuitAction>::new_stateless(move |_| {
            app.quit();
        })
    };
    actions.add_action(quit_action);
    actions.register_for_main_application();

    app.set_accelerators_for_action::<QuitAction>(&["<Control>q"]);

    setup_shortcuts(&app);

    let app = RelmApp::from_app(app).with_broker(&TOASTER_BROKER);
    app.set_global_css(include_str!("styles.less"));
    info!("running application");
    app.visible_on_activate(false).run::<AppModel>(());
    info!("main loop exited");

    Ok(())
}

pub fn setup_shortcuts(_app: &adw::Application) {
    info!("registering application shortcuts...");
    // app.set_accelerators_for_action::<MessagesSearchAction>(&["<Enter>"]);
}
