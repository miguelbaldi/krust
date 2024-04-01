mod imp;

use gtk::{gio, glib, prelude::*};

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Window {
  pub fn new<P: IsA<gtk::Application>>(app: &P) -> Self {
        // Create new window
        glib::Object::builder().property("application", app).build()
    }
}