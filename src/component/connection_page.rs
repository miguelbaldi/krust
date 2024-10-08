#![allow(deprecated)]
// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use std::borrow::Borrow;

use adw::prelude::*;
use gtk::{gdk, gio, Adjustment};
use relm4::{factory::DynamicIndex, *};
use relm4_components::simple_adw_combo_row::{SimpleComboRow, SimpleComboRowMsg};
use tracing::info;

use crate::{
    backend::repository::{KrustConnection, KrustConnectionSecurityType},
    Repository,
};

// Color picker dialog
#[derive(Debug)]
pub struct ColorPickerDialog {}

impl SimpleComponent for ColorPickerDialog {
    type Init = ();
    type Widgets = gtk::ColorDialog;
    type Input = ();
    type Output = ();
    type Root = gtk::ColorDialog;

    fn init_root() -> Self::Root {
        gtk::ColorDialog::builder().build()
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
        let parent = &relm4::main_application().active_window().unwrap();
        let cancellable: Option<&gio::Cancellable> = None;
        dialog.choose_rgba(Some(parent), None, cancellable, |selected_color| {
            info!("color::{:?}", selected_color);
        });
    }
}

// Color picker dialog

#[derive(Debug)]
pub struct ConnectionPageModel {
    pub current_index: Option<DynamicIndex>,
    pub current: Option<KrustConnection>,
    name: String,
    brokers_list: String,
    security_type: KrustConnectionSecurityType,
    sasl_mechanism: String,
    sasl_username: String,
    sasl_password: String,
    security_type_combo: Controller<SimpleComboRow<KrustConnectionSecurityType>>,
    color_picker_dialog: Controller<ColorPickerDialog>,
    timeout: Option<f64>,
}

#[derive(Debug)]
pub enum ConnectionPageMsg {
    New,
    Save,
    Edit(DynamicIndex, KrustConnection),
    SecurityTypeChanged(usize),
}
#[derive(Debug)]
pub enum ConnectionPageOutput {
    Save(Option<DynamicIndex>, KrustConnection),
}

#[relm4::component(pub)]
impl Component for ConnectionPageModel {
    type CommandOutput = ();

    type Init = Option<KrustConnection>;
    type Input = ConnectionPageMsg;
    type Output = ConnectionPageOutput;

    view! {
        #[root]
        adw::PreferencesDialog {
            set_title: "Connection",
            set_height_request: 570,
            add = &adw::PreferencesPage {
                add = &adw::PreferencesGroup {
                    #[name = "name_entry" ]
                    adw::EntryRow {
                        set_title: "Name",
                        set_text: model.name.as_str(),
                    },
                    #[name = "brokers_entry" ]
                    adw::EntryRow {
                        set_title: "Brokers",
                        set_text: model.brokers_list.as_str(),
                    },
                    model.security_type_combo.widget() -> &adw::ComboRow {
                        set_title: "Security type",
                        set_subtitle: "Select security type",
                        set_use_subtitle: true,
                    },
                    #[name = "sasl_mechanism_entry" ]
                    adw::EntryRow {
                        set_title: "SASL mechanism",
                        set_text: model.sasl_mechanism.as_str(),
                    },
                    #[name = "sasl_username_entry" ]
                    adw::EntryRow {
                        set_title: "SASL username",
                        set_text: model.sasl_username.as_str(),
                    },
                    #[name = "sasl_password_entry" ]
                    adw::PasswordEntryRow {
                        set_title: "SASL password",
                        set_text: model.sasl_password.as_str(),
                    },
                    #[name = "color_button" ]
                    gtk::ColorDialogButton {
                        set_dialog = model.color_picker_dialog.widget() -> &gtk::ColorDialog {},
                        connect_rgba_notify => move |btn| {
                            info!("color changed::{}", btn.rgba().to_str());

                        },
                     },
                    #[name = "timeout_entry"]
                    adw::SpinRow {
                        set_title: "Timeout",
                        set_subtitle: "Connection timeout in seconds",
                        set_selectable: true,
                        set_activatable: true,
                        set_focusable: true,
                        set_focus_on_click: true,
                        set_snap_to_ticks: false,
                        set_numeric: true,
                        set_wrap: false,
                        // set_value: model.timeout.unwrap_or_default(),
                    },
                    gtk::Button {
                        set_label: "Save",
                        add_css_class: "suggested-action",
                        set_vexpand: true,
                        set_valign: gtk::Align::End,
                        set_margin_top: 20,
                        connect_clicked[sender] => move |_btn| {
                            sender.input(ConnectionPageMsg::Save)
                        },
                    },
                },
            },
        }
    }

    fn init(
        current_connection: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let default_idx = 0;
        let security_type = SimpleComboRow::builder()
            .launch(SimpleComboRow {
                variants: KrustConnectionSecurityType::VALUES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(
                sender.input_sender(),
                ConnectionPageMsg::SecurityTypeChanged,
            );
        //let security_type_combo = security_type.widget();
        let current = current_connection.clone();
        let color_picker_dialog = ColorPickerDialog::builder().launch(()).detach();

        let model = ConnectionPageModel {
            current_index: None,
            name: current
                .borrow()
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            brokers_list: current
                .borrow()
                .as_ref()
                .map(|c| c.brokers_list.clone())
                .unwrap_or_default(),
            security_type: current
                .borrow()
                .as_ref()
                .map(|c| c.security_type.clone())
                .unwrap_or_default(),
            security_type_combo: security_type,
            sasl_mechanism: current
                .borrow()
                .as_ref()
                .map(|c| c.sasl_mechanism.clone().unwrap_or_default())
                .unwrap_or_default(),
            sasl_username: current
                .borrow()
                .as_ref()
                .map(|c| c.sasl_username.clone().unwrap_or_default())
                .unwrap_or_default(),
            sasl_password: current
                .borrow()
                .as_ref()
                .map(|c| c.sasl_password.clone().unwrap_or_default())
                .unwrap_or_default(),
            current: current_connection,
            color_picker_dialog,
            timeout: current
                .borrow()
                .as_ref()
                .map(|c| c.timeout.map(|t| t as f64))
                .unwrap_or_default(),
        };
        //let security_type_combo = model.security_type_combo.widget();
        let widgets = view_output!();
        model.security_type_combo.widget().queue_allocate();
        let adjustment = Adjustment::builder()
            .lower(0.0)
            .upper(1800.0)
            .page_size(0.0)
            .step_increment(10.0)
            .value(model.timeout.unwrap_or_default())
            .build();
        widgets.timeout_entry.set_adjustment(Some(&adjustment));
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: ConnectionPageMsg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        info!("received message: {:?}", msg);

        match msg {
            ConnectionPageMsg::SecurityTypeChanged(_idx) => {
                let sec_type = match self.security_type_combo.model().get_active_elem() {
                    Some(opt) => opt.clone(),
                    None => KrustConnectionSecurityType::default(),
                };
                self.security_type = sec_type;
                let sasl_visible = match &self.security_type {
                    KrustConnectionSecurityType::PLAINTEXT => false,
                    KrustConnectionSecurityType::SASL_PLAINTEXT => true,
                };
                widgets.sasl_mechanism_entry.set_visible(sasl_visible);
                widgets.sasl_username_entry.set_visible(sasl_visible);
                widgets.sasl_password_entry.set_visible(sasl_visible);
            }
            ConnectionPageMsg::New => {
                widgets.name_entry.set_text("");
                widgets.brokers_entry.set_text("");
                widgets.sasl_mechanism_entry.set_text("");
                widgets.sasl_username_entry.set_text("");
                widgets.sasl_password_entry.set_text("");
                widgets.sasl_mechanism_entry.set_visible(false);
                widgets.sasl_username_entry.set_visible(false);
                widgets.sasl_password_entry.set_visible(false);
                self.security_type_combo
                    .sender()
                    .emit(SimpleComboRowMsg::SetActiveIdx(0));
                self.name = String::default();
                self.brokers_list = String::default();
                self.security_type = KrustConnectionSecurityType::default();
                self.sasl_mechanism = String::default();
                self.sasl_username = String::default();
                self.sasl_password = String::default();
                self.current = None;
                self.current_index = None;
                root.queue_allocate();
                let parent = &relm4::main_application().active_window().unwrap();
                root.present(parent);
            }
            ConnectionPageMsg::Save => {
                let name = widgets.name_entry.text().to_string();
                let brokers_list = widgets.brokers_entry.text().to_string();
                let sasl_mechanism = match widgets.sasl_mechanism_entry.text().as_str() {
                    "" => None,
                    vstr => Some(vstr.to_string()),
                };
                let sasl_username = match widgets.sasl_username_entry.text().as_str() {
                    "" => None,
                    vstr => Some(vstr.to_string()),
                };
                let sasl_password = match widgets.sasl_password_entry.text().as_str() {
                    "" => None,
                    vstr => Some(vstr.to_string()),
                };
                let security_type = self.security_type.clone();
                let color = widgets.color_button.rgba();
                info!("selected color::{:?}", color);
                let timeout = widgets.timeout_entry.value() as usize;
                let timeout = if timeout < 1 { None } else { Some(timeout) };
                widgets.name_entry.set_text("");
                widgets.brokers_entry.set_text("");
                widgets.sasl_username_entry.set_text("");
                widgets.sasl_password_entry.set_text("");
                widgets.timeout_entry.set_value(0.0);
                sender
                    .output(ConnectionPageOutput::Save(
                        self.current_index.clone(),
                        KrustConnection {
                            id: (move |current: Option<KrustConnection>| current?.id)(
                                self.current.clone(),
                            ),
                            name,
                            brokers_list,
                            sasl_username,
                            sasl_password,
                            sasl_mechanism,
                            security_type,
                            color: Some(color.to_string()),
                            timeout,
                        },
                    ))
                    .unwrap();
                root.close();
            }
            ConnectionPageMsg::Edit(index, connection) => {
                let idx = Some(index.clone());
                let connection = Repository::new()
                    .connection_by_id(connection.id.unwrap())
                    .unwrap();
                let color = connection
                    .color
                    .clone()
                    .unwrap_or("rgb(183, 243, 155)".to_string());
                let color = gdk::RGBA::parse(color).expect("Should return RGBA color");
                widgets.color_button.set_rgba(&color);
                let conn = connection.clone();
                self.current_index = idx;
                self.current = Some(connection);
                self.name = conn.name;
                self.brokers_list = conn.brokers_list;
                self.security_type = conn.security_type.clone();
                self.sasl_mechanism = conn.sasl_mechanism.unwrap_or_default();
                self.sasl_username = conn.sasl_username.unwrap_or_default();
                self.sasl_password = conn.sasl_password.unwrap_or_default();
                self.timeout = conn.timeout.map(|t| t as f64);
                widgets.name_entry.set_text(self.name.clone().as_str());
                widgets
                    .brokers_entry
                    .set_text(self.brokers_list.clone().as_str());
                widgets.sasl_mechanism_entry.set_text(&self.sasl_mechanism);
                let combo_idx = KrustConnectionSecurityType::VALUES
                    .iter()
                    .position(|v| *v == self.security_type)
                    .expect("Should return option index");
                info!("connection_dialog::security_type::index::{}", combo_idx);
                self.security_type_combo
                    .sender()
                    .emit(SimpleComboRowMsg::SetActiveIdx(combo_idx));
                widgets
                    .sasl_username_entry
                    .set_text(self.sasl_username.clone().as_str());
                widgets
                    .sasl_password_entry
                    .set_text(self.sasl_password.clone().as_str());
                let sasl_visible = match &self.security_type {
                    KrustConnectionSecurityType::PLAINTEXT => false,
                    KrustConnectionSecurityType::SASL_PLAINTEXT => true,
                };
                widgets.sasl_mechanism_entry.set_sensitive(sasl_visible);
                widgets.sasl_username_entry.set_sensitive(sasl_visible);
                widgets.sasl_password_entry.set_sensitive(sasl_visible);
                widgets
                    .timeout_entry
                    .set_value(self.timeout.unwrap_or_default());
                root.queue_allocate();
                let parent = &relm4::main_application().active_window().unwrap();
                root.present(parent);
            }
        };

        self.update_view(widgets, sender);
    }
}
