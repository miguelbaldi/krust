use gtk::prelude::*;
use relm4::{
  factory::{DynamicIndex, FactoryComponent},
  FactorySender,
};
use tracing::info;

use crate::backend::repository::KrustConnection;


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
  pub security_type: Option<String>,
  pub sasl_mechanism: Option<String>,
  pub jaas_config: Option<String>,
}

impl From<&mut ConnectionListModel> for KrustConnection {
    fn from(value: &mut ConnectionListModel) -> Self {
      KrustConnection {
        id: value.id.clone(),
        name: value.name.clone(),
        brokers_list: value.brokers_list.clone(),
        security_type: value.security_type.clone(),
        sasl_mechanism: value.sasl_mechanism.clone(),
        jaas_config: value.jaas_config.clone(),
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
        add_css_class: "connection-toggle",
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
      jaas_config: conn.jaas_config,
    }
  }
  
  fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
    match msg {
      KrustConnectionMsg::Connect => {
        info!("Connect request for {}", self.name);
        // let kafka = KafkaBackend::new(KrustConnection {
        //   id: self.id,
        //   name: self.name.clone(),
        //   brokers_list: self.brokers_list.clone(),
        //   security_type: self.security_type.clone(),
        //   sasl_mechanism: self.sasl_mechanism.clone(),
        //   jaas_config: self.jaas_config.clone(),
        // });
        // let topics = kafka.list_topics();
        // for topic in topics {
        //   debug!("TOPIC::{} ({})", topic.name, topic.partitions.len());
        // }
        let conn: KrustConnection = self.into();
        sender
        .output(KrustConnectionOutput::ShowTopics(conn))
        .unwrap();
      }
      KrustConnectionMsg::Disconnect => {
        info!("Disconnect request for {}", self.name);
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
