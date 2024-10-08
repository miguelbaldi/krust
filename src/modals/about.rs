// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use fs_extra::dir::get_size;
use gtk::prelude::GtkWindowExt;
use humansize::{format_size, DECIMAL};
use relm4::{adw, gtk, ComponentParts, ComponentSender, SimpleComponent};
use sysinfo::Disks;
use tracing::*;

use crate::{Settings, APP_ID, APP_NAME, VERSION};

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
            .license_type(gtk::License::Gpl30)
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
        let ack = &[
            "Adelar Escobar Vieira",
            "Francivaldo Napoleão Herculano",
            "Jessica dos Santos Rodrigues",
        ];
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
        let disks = Disks::new_with_refreshed_list();
        let settings = Settings::read().unwrap_or_default();
        let cache_dir_size = format_size(get_size(settings.cache_dir).unwrap_or(0), DECIMAL);
        info!("[DISK] Cache directory size: {}", cache_dir_size);
        for disk in disks.list() {
            info!(
                "[DISK]{:?}: {:?}:{:?} / {}",
                disk.name(),
                disk.kind(),
                disk.mount_point(),
                format_size(disk.total_space(), DECIMAL),
            );
        }
        dialog.present();
    }
}
