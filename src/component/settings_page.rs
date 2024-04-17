use std::path::PathBuf;

use gtk::prelude::*;
use relm4::*;
use relm4_components::open_dialog::{OpenDialog, OpenDialogMsg, OpenDialogResponse, OpenDialogSettings};
use tracing::{debug, info};

use crate::backend::settings::Settings;

#[derive(Debug)]
pub struct SettingsPageModel {
    cache_dir: String,
    cache_dir_dialog: Controller<OpenDialog>,
}

#[derive(Debug)]
pub enum SettingsPageMsg {
    Save,
    Edit,
    ChooseCacheDirRequest,
    OpenCacheDir(PathBuf),
    Ignore,
}
#[derive(Debug)]
pub enum SettingsPageOutput {
    Saved,
}

#[relm4::component(pub)]
impl Component for SettingsPageModel {
    type CommandOutput = ();

    type Init = ();
    type Input = SettingsPageMsg;
    type Output = SettingsPageOutput;

    view! {
      #[root]
      gtk::Grid {
        set_margin_all: 10,
        set_row_spacing: 6,
        set_column_spacing: 10,
        attach[0,0,1,2] = &gtk::Label {
          set_label: "Cache location"
        },
        attach[1,0,1,2]: cache_dir_entry = &gtk::Entry {
          set_hexpand: true,
          #[watch]
          set_text: model.cache_dir.as_str(),
          set_sensitive: false,
        },
        attach[2,0,1,2]: open_cache_dir_dialog_button = &gtk::Button {
            set_icon_name: "document-open-symbolic",
            connect_clicked => SettingsPageMsg::ChooseCacheDirRequest,
        },
        attach[1,100,1,2] = &gtk::Button {
            set_label: "Save",
            add_css_class: "suggested-action",
            connect_clicked[sender] => move |_btn| {
              sender.input(SettingsPageMsg::Save)
            },
          },
      }
    }

    fn init(
        _: Self::Init,
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
            OpenDialogResponse::Accept(path) => SettingsPageMsg::OpenCacheDir(path),
            OpenDialogResponse::Cancel => SettingsPageMsg::Ignore,
        });
        let model = SettingsPageModel {
            cache_dir: current.cache_dir,
            cache_dir_dialog,
        };
        //let security_type_combo = model.security_type_combo.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: SettingsPageMsg,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        info!("received message: {:?}", msg);

        match msg {
            SettingsPageMsg::Ignore => {}
            SettingsPageMsg::ChooseCacheDirRequest => {
                self.cache_dir_dialog.emit(OpenDialogMsg::Open);
            }
            SettingsPageMsg::OpenCacheDir(path_buff) => {
                match path_buff.as_path().to_str() {
                    Some(path) => {
                        info!("cache dir path selected: {}", path);
                        self.cache_dir = path.to_string();
                    }
                    None => debug!("did not selected any path")
                };
            }
            SettingsPageMsg::Save => {
                let cache_dir = widgets.cache_dir_entry.text().to_string();
                Settings {
                    cache_dir,
                }
                .write()
                .expect("should write current settings");
                sender.output(SettingsPageOutput::Saved).unwrap();
            }
            SettingsPageMsg::Edit => {
                let current_settings = Settings::read().unwrap_or_default();
                self.cache_dir = current_settings.cache_dir;
                widgets
                    .cache_dir_entry
                    .set_text(self.cache_dir.clone().as_str());
            }
        };

        self.update_view(widgets, sender);
    }
}
