use gtk::subclass::prelude::*;
use gtk::glib;


// Object holding the state
#[derive(Debug, Default, gtk::CompositeTemplate)]
#[template(resource = "/org/miguelbaldi/krust/application.ui")]
pub struct Window {
    #[template_child(id = "stMain")]
    pub st_main: TemplateChild<gtk::Stack>,
    #[template_child(id = "stkDefaultPage")]
    pub st_default_page: TemplateChild<gtk::StackPage>,
    #[template_child(id = "stkClusterPage")]
    pub st_cluster_page: TemplateChild<gtk::StackPage>,
}

// The central trait for subclassing a GObject
#[glib::object_subclass]
impl ObjectSubclass for Window {
    // `NAME` needs to match `class` attribute of template
    const NAME: &'static str = "MainWindow";
    type Type = super::Window;
    type ParentType = gtk::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

// Trait shared by all GObjects
impl ObjectImpl for Window {
  fn constructed(&self) {
      // Call "constructed" on parent
      self.parent_constructed();
      self.st_main.set_visible_child_name("stkClusterPage")

      // Connect to "clicked" signal of `button`
      // self.boxLayout.connect_clicked(move |button| {
      //     // Set the label to "Hello World!" after the button has been clicked on
      //     button.set_label("Hello World!");
      // });
  }
}

// ANCHOR_END: object_impl

// Trait shared by all widgets
impl WidgetImpl for Window {}

// Trait shared by all windows
impl WindowImpl for Window {}

// Trait shared by all application windows
impl ApplicationWindowImpl for Window {}