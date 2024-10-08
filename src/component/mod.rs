// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

//! Relm4 components.

use crate::backend::repository::KrustConnection;
use adw::TabBar;
use gtk::prelude::*;
use tracing::*;
pub mod app;
pub mod messages;
pub mod topics;

pub(crate) mod cache_manager_dialog;
pub(crate) mod connection_list;
mod connection_page;
pub(crate) mod settings_dialog;
mod status_bar;
pub(crate) mod task_manager;

pub fn get_tab_by_title(tabbar: &TabBar, title: String) -> Option<gtk::Widget> {
    let tab_revealer: gtk::Revealer = tabbar.first_child().and_downcast().unwrap();
    let tab_box: gtk::Box = tab_revealer.child().and_downcast().unwrap();
    let tab_sw: gtk::ScrolledWindow = tab_box.observe_children().item(2).and_downcast().unwrap();
    let tab_box: gtk::Widget = tab_sw.first_child().and_downcast().unwrap();
    let mut maybe_tab: Option<gtk::Widget> = None;
    for i in 0..tab_box.observe_children().n_items() {
        let tab_gizmo: gtk::Widget = tab_box.observe_children().item(i).and_downcast().unwrap();
        if tab_gizmo.css_name() == "tabboxchild" {
            let tab: gtk::Widget = tab_gizmo.first_child().and_downcast().unwrap();
            if tab.css_name() == "tab" {
                let ttitle: gtk::Widget = tab.observe_children().item(1).and_downcast().unwrap();
                let ttitle: gtk::Label = ttitle.first_child().and_downcast().unwrap();
                let ttitle = ttitle.label();
                debug!("get_tab_by_title::title={}", ttitle);
                if ttitle == title {
                    maybe_tab = Some(tab);
                }
            }
        }
    }
    maybe_tab
}

pub fn colorize_widget_by_connection(conn: &KrustConnection, widget: gtk::Widget) {
    info!("color_widget_by_connection::{:?}", widget);
    let css_provider = gtk::CssProvider::new();
    let color = conn
        .clone()
        .color
        .clone()
        .unwrap_or("rgb(183, 243, 155)".to_string());
    let color = color.as_str();
    let css_class = format!("custom_color_{}", conn.id.unwrap());
    css_provider.load_from_string(format!(".{} {{ background: {};}}", css_class, color).as_str());
    let display = widget.display();
    gtk::style_context_add_provider_for_display(
        &display,
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    widget.add_css_class(&css_class);
}
