use gtk::gdk;
//#![windows_subsystem = "windows"]
use gtk::prelude::ApplicationExt;
use gtk::gio;
use tracing::*;
use tracing_subscriber::filter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_tree::HierarchicalLayer;

use relm4::{
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    gtk, main_application, RelmApp,
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
    // Call `gtk::init` manually because we instantiate GTK types in the app model.
    gtk::init().expect("should initialize GTK");
    let filter = filter::Targets::new()
        // Enable the `INFO` level for anything in `my_crate`
        .with_target("relm4", Level::INFO)
        // Enable the `DEBUG` level for a specific module.
        .with_target("krust", Level::DEBUG);
    tracing_subscriber::registry()
        .with(HierarchicalLayer::new(2))
        .with(EnvFilter::from_default_env())
        .with(filter)
        .init();

    info!("starting application: {}", APP_ID);
    initialize_resources();
    gtk::Window::set_default_icon_name("krust-icon");
    let app = main_application();
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

    let app = RelmApp::from_app(app);
    app.set_global_css(include_str!("styles.css"));
    info!("running application");
    app.visible_on_activate(false).run::<AppModel>(());
    info!("main loop exited");

    Ok(())
}
