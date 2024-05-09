use std::path::PathBuf;

use adw::prelude::*;
use relm4::{gtk, Component, ComponentController, ComponentParts, ComponentSender, Controller};
use relm4_components::open_dialog::{
    OpenDialog, OpenDialogMsg, OpenDialogResponse, OpenDialogSettings,
};
use tracing::*;

use crate::backend::settings::Settings;

pub struct SettingsDialogModel {
    cache_dir: String,
    cache_dir_dialog: Controller<OpenDialog>,
    is_full_timestamp: bool,
}

#[derive(Debug)]
pub enum SettingsDialogMsg {
    Show,
    Save,
    ChooseCacheDirRequest,
    OpenCacheDir(PathBuf),
    SwitchFullTimestamp,
    Ignore,
}

pub struct SettingsDialogInit {}

#[relm4::component(pub)]
impl Component for SettingsDialogModel {
    type CommandOutput = ();
    type Input = SettingsDialogMsg;
    type Output = ();
    type Init = SettingsDialogInit;

    view! {
        #[root]
        adw::PreferencesDialog {
            set_title: "Preferences",
            add = &adw::PreferencesPage {
                set_title: "Messages",
                set_name: Some("Messages"),
                set_icon_name: Some("emblem-system-symbolic"),
                add = &adw::PreferencesGroup {
                    set_title: "General",
                    #[name = "is_full_timestamp_row"]
                    adw::SwitchRow {
                        set_title: "Full timestamp",
                        set_subtitle: "Show message timestamp with milliseconds",
                        set_active: model.is_full_timestamp,
                        connect_active_notify => SettingsDialogMsg::SwitchFullTimestamp,
                    }
                },
                add = &adw::PreferencesGroup {
                    set_title: "Caching",
                    #[name = "cache_location_row"]
                    adw::ActionRow {
                        set_title: "Location",
                        #[watch]
                        set_subtitle: &model.cache_dir,
                        add_suffix: open_cache_dir_dialog_button = &gtk::Button {
                            set_icon_name: "document-open-symbolic",
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            set_hexpand: false,
                            set_vexpand: false,
                            connect_clicked => SettingsDialogMsg::ChooseCacheDirRequest,
                        },
                    }
                },
            },
            add = &adw::PreferencesPage {
                set_title: "Topics",
                set_name: Some("Topics"),
                set_icon_name: Some("emblem-system-symbolic"),
                add = &adw::PreferencesGroup {
                    set_title: "General",
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current = Settings::read().unwrap_or_default();
        let cache_dir_dialog = OpenDialog::builder()
            .transient_for_native(&root)
            .launch(OpenDialogSettings {
                folder_mode: true,
                accept_label: String::from("Select"),
                cancel_label: String::from("Cancel"),
                create_folders: true,
                is_modal: true,
                filters: Vec::new(),
            })
            .forward(sender.input_sender(), |response| match response {
                OpenDialogResponse::Accept(path) => SettingsDialogMsg::OpenCacheDir(path),
                OpenDialogResponse::Cancel => SettingsDialogMsg::Ignore,
            });
        let model = SettingsDialogModel {
            cache_dir: current.cache_dir,
            cache_dir_dialog,
            is_full_timestamp: current.is_full_timestamp,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            SettingsDialogMsg::Show => {
                let parent = &relm4::main_application().active_window().unwrap();
                let current_settings = Settings::read().unwrap_or_default();
                self.cache_dir = current_settings.cache_dir;
                self.is_full_timestamp = current_settings.is_full_timestamp;
                root.queue_allocate();
                root.present(parent);
            }
            SettingsDialogMsg::Ignore => {}
            SettingsDialogMsg::ChooseCacheDirRequest => {
                self.cache_dir_dialog.emit(OpenDialogMsg::Open);
            }
            SettingsDialogMsg::OpenCacheDir(path_buff) => {
                match path_buff.as_path().to_str() {
                    Some(path) => {
                        info!("cache dir path selected: {}", path);
                        self.cache_dir = path.to_string();
                        widgets.cache_location_row.set_subtitle(&self.cache_dir);
                        sender.input(SettingsDialogMsg::Save);
                    }
                    None => debug!("did not selected any path"),
                };
            }
            SettingsDialogMsg::SwitchFullTimestamp => {
                self.is_full_timestamp = widgets.is_full_timestamp_row.is_active();
                sender.input(SettingsDialogMsg::Save);
            }
            SettingsDialogMsg::Save => {
                let cache_dir = self.cache_dir.clone();
                let settings = Settings { cache_dir: cache_dir, is_full_timestamp: self.is_full_timestamp };
                info!("settings_dialog::saving::{:?}", settings);
                settings
                    .write()
                    .expect("should write current settings");
            }
        }
    }
}
