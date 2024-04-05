
use gtk::prelude::*;
use relm4::{factory::DynamicIndex, *};
use tracing::info;

use crate::backend::repository::KrustConnection;

#[derive(Debug, Default)]
pub struct ConnectionPageModel {
  pub current_index: Option<DynamicIndex>,
  pub current: Option<KrustConnection>,
  name: String,
  brokers_list: String,
}

#[derive(Debug)]
pub enum ConnectionPageMsg {
  New,
  Save,
  Edit(DynamicIndex, KrustConnection),
}
#[derive(Debug)]
pub enum ConnectionPageOutput {
  Save(Option<DynamicIndex>, KrustConnection),
}

fn model_from(idx: Option<DynamicIndex>, current: Option<KrustConnection>) -> ConnectionPageModel {
  let (name, brokers_list) = match current.clone() {
    Some(conn) => (conn.name, conn.brokers_list),
    None => ("".into(), "".into()),
  };
  
  ConnectionPageModel {
    current_index: idx,
    current,
    name,
    brokers_list,
  }
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
        #[watch]
        set_text: model.name.as_str(),
      },
      attach[0,4,1,2] = &gtk::Label {
        set_label: "Brokers"
      },
      attach[1,4,1,2]: brokers_entry = &gtk::Entry {
        set_hexpand: true,
        #[watch]
        set_text: model.brokers_list.as_str(),
      },
      attach[1,16,1,2] = &gtk::Button {
        set_label: "Save",
        add_css_class: "suggested-action",
        connect_clicked[sender] => move |_btn| {
          sender.input(ConnectionPageMsg::Save)
        },
      },
    }
  }
  
  fn init(
    current: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let model = model_from(None, current);
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
      ConnectionPageMsg::New => {
        widgets.name_entry.set_text("");
        widgets.brokers_entry.set_text("");
        self.name = String::default();
        self.brokers_list = String::default();
        self.current = None;
        self.current_index = None;
      }
      ConnectionPageMsg::Save => {
        let name = widgets.name_entry.text().to_string();
        let brokers_list = widgets.brokers_entry.text().to_string();
        widgets.name_entry.set_text("");
        widgets.brokers_entry.set_text("");
        sender
        .output(ConnectionPageOutput::Save(self.current_index.clone(), KrustConnection {
          id: (move |current: Option<KrustConnection>| current?.id)(
            self.current.clone(),
          ),
          name: name,
          brokers_list: brokers_list,
          jaas_config: None,
          sasl_mechanism: None,
          security_type: None,
        }))
        .unwrap();
      }
      ConnectionPageMsg::Edit(index, conn) => {
        let idx = Some(index.clone());
        let model = model_from(idx, Some(conn));
        self.current_index = model.current_index;
        self.current = model.current;
        self.name = model.name;
        self.brokers_list = model.brokers_list;
        //mem::swap(self, &mut model_from(idx, Some(conn)));
      }
    };
    
    self.update_view(widgets, sender);
  }
}
