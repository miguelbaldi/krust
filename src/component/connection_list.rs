use gtk::prelude::*;
use relm4::{
    factory::{DynamicIndex, FactoryComponent},
    FactorySender,
};
use tracing::info;

use crate::backend::repository::{KrustConnection, KrustConnectionSecurityType};

#[derive(Debug)]
pub enum KrustConnectionMsg {
    Connect,
    Disconnect,
    Edit(DynamicIndex),
}

#[derive(Debug)]
pub enum KrustConnectionOutput {
    Add,
    Edit(DynamicIndex, KrustConnection),
    Remove(DynamicIndex),
    ShowTopics(KrustConnection),
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionListModel {
    pub id: Option<usize>,
    pub name: String,
    pub brokers_list: String,
    pub security_type: KrustConnectionSecurityType,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub is_connected: bool,
}

impl From<&mut ConnectionListModel> for KrustConnection {
    fn from(value: &mut ConnectionListModel) -> Self {
        KrustConnection {
            id: value.id.clone(),
            name: value.name.clone(),
            brokers_list: value.brokers_list.clone(),
            security_type: value.security_type.clone(),
            sasl_mechanism: value.sasl_mechanism.clone(),
            sasl_username: value.sasl_username.clone(),
            sasl_password: value.sasl_password.clone(),
        }
    }
}
#[relm4::factory(pub)]
impl FactoryComponent for ConnectionListModel {
    type Init = KrustConnection;
    type Input = KrustConnectionMsg;
    type Output = KrustConnectionOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
      #[root]
      gtk::Box {
        set_orientation: gtk::Orientation::Horizontal,
        set_spacing: 10,

        #[name(connect_button)]
        gtk::ToggleButton {
          set_label: "Connect",
          add_css_class: "krust-toggle",
          connect_toggled[sender] => move |btn| {
            if btn.is_active() {
              sender.input(KrustConnectionMsg::Connect);
            } else {
              sender.input(KrustConnectionMsg::Disconnect);
            }
          },
        },
        gtk::Button {
          set_icon_name: "emblem-system-symbolic",
          connect_clicked[sender, index] => move |_| {
            sender.input(KrustConnectionMsg::Edit(index.clone()));
          },
        },
        #[name(label)]
        gtk::Label {
          #[watch]
          set_label: &self.name,
          set_width_chars: 3,
        },
      }
    }

    fn init_model(conn: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self {
            id: conn.id,
            name: conn.name,
            brokers_list: conn.brokers_list,
            security_type: conn.security_type,
            sasl_mechanism: conn.sasl_mechanism,
            sasl_username: conn.sasl_username,
            sasl_password: conn.sasl_password,
            is_connected: false,
        }
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            KrustConnectionMsg::Connect => {
                info!("Connect request for {}", self.name);
                self.is_connected = true;
                let conn: KrustConnection = self.into();
                sender
                    .output(KrustConnectionOutput::ShowTopics(conn))
                    .unwrap();
            }
            KrustConnectionMsg::Disconnect => {
                info!("Disconnect request for {}", self.name);
                self.is_connected = false;
            }
            KrustConnectionMsg::Edit(index) => {
                info!("Edit request for {}", self.name);
                sender
                    .output(KrustConnectionOutput::Edit(index, self.into()))
                    .unwrap();
            }
        }
    }
}
