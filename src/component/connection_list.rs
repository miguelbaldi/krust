use gtk::prelude::*;
use relm4::{
  factory::{DynamicIndex, FactoryComponent},
  FactorySender,
};
use tracing::info;

#[derive(Debug)]
pub enum ConnectionMsg {
  Connect,
  Disconnect,
}

#[derive(Debug)]
pub enum ConnectionOutput {
  Add(Connection),
  Remove(DynamicIndex),
}

#[derive(Debug)]
pub struct Connection {
  pub name: String,
}

#[relm4::factory(pub)]
impl FactoryComponent for Connection {
  type Init = String;
  type Input = ConnectionMsg;
  type Output = ConnectionOutput;
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
        connect_toggled[sender] => move |btn| {
          if btn.is_active() {
            sender.input(ConnectionMsg::Connect);
          } else {
            sender.input(ConnectionMsg::Disconnect);
          }
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
  
  fn init_model(name: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
    Self { name }
  }
  
  fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
    match msg {
      ConnectionMsg::Connect => {
        info!("Connected {}", self.name);
      }
      ConnectionMsg::Disconnect => {
        info!("Disconnected {}", self.name);
      }
    }
  }
}
