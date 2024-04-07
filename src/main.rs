//#![windows_subsystem = "windows"]
use gtk::prelude::ApplicationExt;
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

fn main() -> Result<(), ()> {
    // Call `gtk::init` manually because we instantiate GTK types in the app model.
    gtk::init().unwrap();
    let filter = filter::Targets::new()
        // Enable the `INFO` level for anything in `my_crate`
        .with_target("relm4", Level::WARN)
        // Enable the `DEBUG` level for a specific module.
        .with_target("krust", Level::DEBUG);
    tracing_subscriber::registry()
        .with(HierarchicalLayer::new(2))
        .with(EnvFilter::from_default_env())
        .with(filter)
        .init();

    info!("Running: {}", APP_ID);

    let app = main_application();

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

    let mut repo = Repository::new();
    repo.init().unwrap();

    //let app = RelmApp::new(APP_ID);
    app.set_global_css(include_str!("styles.css"));
    //app.run::<AppModel>(());
    app.visible_on_activate(false).run::<AppModel>(());
    info!("main loop exited");

    Ok(())
}
