use relm4::*;
use tracing::*;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_tree::HierarchicalLayer;

use krust::{AppModel, AppMode, APP_ID};

fn main() -> Result<(), ()> {
  tracing_subscriber::registry()
  .with(HierarchicalLayer::new(2))
  .with(EnvFilter::from_default_env())
  .init();
  
  info!("Running: {}", APP_ID);
  
  // Call `gtk::init` manually because we instantiate GTK types in the app model.
  gtk::init().unwrap();
  
  let app = RelmApp::new(APP_ID);
  app.run::<AppModel>(AppMode::View);
  
  info!("main loop exited");
  
  Ok(())
}