use gtk::prelude::GtkWindowExt;
use relm4::{adw, gtk, ComponentParts, ComponentSender, SimpleComponent};

use crate::{APP_ID, APP_NAME, VERSION};

#[derive(Debug)]
pub struct AboutDialog {}

impl SimpleComponent for AboutDialog {
    type Init = ();
    type Widgets = adw::AboutWindow;
    type Input = ();
    type Output = ();
    type Root = adw::AboutWindow;

    fn init_root() -> Self::Root {
        let about = adw::AboutWindow::builder()
            //.application_icon("/org/miguelbaldi/krust/logo.png")
            .application_icon(APP_ID)
            // Insert your license of choice here
            .license_type(gtk::License::MitX11)
            // Insert your website here
            .website("https://github.com/miguelbaldi/krust")
            // Insert your Issues page
            .issue_url("https://github.com/miguelbaldi/krust/issues")
            // Insert your application name here
            .application_name(APP_NAME)
            .version(VERSION)
            .copyright("© 2024 Miguel A. Baldi Hörlle")
            .developers(vec!["Miguel A. Baldi Hörlle"])
            .designers(vec!["Miguel A. Baldi Hörlle"])
            .hide_on_close(true)
            .build();
        let ack = &["Adelar Escobar Vieira", "Francivaldo Napoleão Herculano", "Jessica dos Santos Rodrigues"];
        about.add_acknowledgement_section(Some("Special thanks to"), ack);
        about
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {};

        let widgets = root.clone();

        ComponentParts { model, widgets }
    }

    fn update_view(&self, dialog: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        dialog.present();
    }
}
