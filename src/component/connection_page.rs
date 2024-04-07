use std::borrow::Borrow;

use gtk::prelude::*;
use relm4::{factory::DynamicIndex, *};
use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};
use tracing::info;

use crate::backend::repository::{KrustConnection, KrustConnectionSecurityType};

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
    security_type_combo: Controller<SimpleComboBox<KrustConnectionSecurityType>>,
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
      gtk::Grid {
        set_margin_all: 10,
        set_row_spacing: 6,
        set_column_spacing: 10,
        attach[0,0,1,2] = &gtk::Label {
          set_label: "Name"
        },
        attach[1,0,1,2]: name_entry = &gtk::Entry {
          set_hexpand: true,
          set_text: model.name.as_str(),
        },
        attach[0,4,1,2] = &gtk::Label {
          set_label: "Brokers"
        },
        attach[1,4,1,2]: brokers_entry = &gtk::Entry {
          set_hexpand: true,
          set_text: model.brokers_list.as_str(),
        },
        attach[0,8,1,2] = &gtk::Label {
          set_label: "Security type"
        },
        attach[1,8,1,2] = model.security_type_combo.widget() -> &gtk::ComboBoxText {},
        attach[0,16,1,2] = &gtk::Label {
          set_label: "SASL mechanism"
        },
        attach[1,16,1,2]: sasl_mechanism_entry = &gtk::Entry {
          set_hexpand: true,
          set_text: model.sasl_mechanism.as_str(),
        },
        attach[0,24,1,2] = &gtk::Label {
          set_label: "SASL username"
        },
        attach[1,24,1,2]: sasl_username_entry = &gtk::Entry {
          set_hexpand: true,
          set_text: model.sasl_username.as_str(),
        },
        attach[0,28,1,2] = &gtk::Label {
          set_label: "SASL password"
        },
        attach[1,28,1,2]: sasl_password_entry = &gtk::PasswordEntry {
          set_hexpand: true,
          set_text: model.sasl_password.as_str(),
        },
        attach[1,128,1,2] = &gtk::Button {
          set_label: "Save",
          add_css_class: "suggested-action",
          connect_clicked[sender] => move |_btn| {
            sender.input(ConnectionPageMsg::Save)
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
        let security_type_combo = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: KrustConnectionSecurityType::VALUES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(
                sender.input_sender(),
                ConnectionPageMsg::SecurityTypeChanged,
            );
        let current = current_connection.clone();
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
            security_type_combo: security_type_combo,
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
        };
        //let security_type_combo = model.security_type_combo.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: ConnectionPageMsg,
        sender: ComponentSender<Self>,
        _: &Self::Root,
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
                widgets.sasl_mechanism_entry.set_sensitive(sasl_visible);
                widgets.sasl_username_entry.set_sensitive(sasl_visible);
                widgets.sasl_password_entry.set_sensitive(sasl_visible);
            }
            ConnectionPageMsg::New => {
                widgets.name_entry.set_text("");
                widgets.brokers_entry.set_text("");
                widgets.sasl_mechanism_entry.set_text("");
                widgets.sasl_username_entry.set_text("");
                widgets.sasl_password_entry.set_text("");
                widgets.sasl_mechanism_entry.set_sensitive(false);
                widgets.sasl_username_entry.set_sensitive(false);
                widgets.sasl_password_entry.set_sensitive(false);
                self.security_type_combo
                    .sender()
                    .emit(SimpleComboBoxMsg::SetActiveIdx(0));
                self.name = String::default();
                self.brokers_list = String::default();
                self.security_type = KrustConnectionSecurityType::default();
                self.sasl_mechanism = String::default();
                self.sasl_username = String::default();
                self.sasl_password = String::default();
                self.current = None;
                self.current_index = None;
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
                widgets.name_entry.set_text("");
                widgets.brokers_entry.set_text("");
                widgets.sasl_username_entry.set_text("");
                widgets.sasl_password_entry.set_text("");
                sender
                    .output(ConnectionPageOutput::Save(
                        self.current_index.clone(),
                        KrustConnection {
                            id: (move |current: Option<KrustConnection>| current?.id)(
                                self.current.clone(),
                            ),
                            name: name,
                            brokers_list: brokers_list,
                            sasl_username: sasl_username,
                            sasl_password: sasl_password,
                            sasl_mechanism: sasl_mechanism,
                            security_type: security_type,
                        },
                    ))
                    .unwrap();
            }
            ConnectionPageMsg::Edit(index, connection) => {
                let idx = Some(index.clone());
                let conn = connection.clone();
                self.current_index = idx;
                self.current = Some(connection);
                self.name = conn.name;
                self.brokers_list = conn.brokers_list;
                self.security_type = conn.security_type.clone();
                self.sasl_mechanism = conn.sasl_mechanism.unwrap_or_default();
                self.sasl_username = conn.sasl_username.unwrap_or_default();
                self.sasl_password = conn.sasl_password.unwrap_or_default();
                widgets.name_entry.set_text(self.name.clone().as_str());
                widgets
                    .brokers_entry
                    .set_text(&self.brokers_list.clone().as_str());
                let combo_idx = KrustConnectionSecurityType::VALUES
                    .iter()
                    .position(|v| *v == self.security_type)
                    .unwrap_or_default();
                self.security_type_combo
                    .sender()
                    .emit(SimpleComboBoxMsg::SetActiveIdx(combo_idx));
                widgets
                    .sasl_username_entry
                    .set_text(&&self.sasl_username.clone().as_str());
                widgets
                    .sasl_password_entry
                    .set_text(&&self.sasl_password.clone().as_str());
                let sasl_visible = match &self.security_type {
                    KrustConnectionSecurityType::PLAINTEXT => false,
                    KrustConnectionSecurityType::SASL_PLAINTEXT => true,
                };
                widgets.sasl_mechanism_entry.set_sensitive(sasl_visible);
                widgets.sasl_username_entry.set_sensitive(sasl_visible);
                widgets.sasl_password_entry.set_sensitive(sasl_visible);
                //mem::swap(self, &mut model_from(idx, Some(conn)));
            }
        };

        self.update_view(widgets, sender);
    }
}
