use gtk::prelude::*;
use relm4::{
  typed_view::column::{LabelColumn, TypedColumnView},
  *,
};
use tracing::info;

use crate::backend::repository::{KrustConnection, KrustMessage};

// Table: start
#[derive(Debug, PartialEq, Eq)]
struct MessageListItem {
  offset: i64,
  key: String,
  value: String,
  timestamp: Option<i64>, 
}

impl MessageListItem {
  fn new(value: KrustMessage) -> Self {
    Self {
      offset: value.offset,
      key: "".to_string(),
      value: value.value,
      timestamp: value.timestamp,
    }
  }
}

struct OfssetColumn;

impl LabelColumn for OfssetColumn {
  type Item = MessageListItem;
  type Value = i64;
  
  const COLUMN_NAME: &'static str = "Offset";
  
  const ENABLE_SORT: bool = true;
  const ENABLE_RESIZE: bool = true;
  
  fn get_cell_value(item: &Self::Item) -> Self::Value {
    item.offset
  }
  
  fn format_cell_value(value: &Self::Value) -> String {
    format!("{}", value)
  }
}

struct ValueColumn;

impl LabelColumn for ValueColumn {
  type Item = MessageListItem;
  type Value = String;
  
  const COLUMN_NAME: &'static str = "Value";
  const ENABLE_RESIZE: bool = true;
  const ENABLE_EXPAND: bool = true;
  const ENABLE_SORT: bool = true;
  
  fn get_cell_value(item: &Self::Item) -> Self::Value {
    item.value.clone()
  }
  
  fn format_cell_value(value: &Self::Value) -> String {
    format!("{}...", value.replace("\n", " ").get(0..200).unwrap_or("").to_string())
  }
}

// Table: end

#[derive(Debug)]
pub struct MessagesPageModel {
  pub current: Option<KrustConnection>,
  messages_wrapper: TypedColumnView<MessageListItem, gtk::SingleSelection>,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
  List(Vec<KrustMessage>),
}

#[derive(Debug)]
pub enum MessagesPageOutput {
  ShowMessages,
}

#[relm4::component(pub)]
impl Component for MessagesPageModel {
  type CommandOutput = ();
  
  type Init = Option<KrustConnection>;
  type Input = MessagesPageMsg;
  type Output = ();
  
  view! {
    #[root]
    gtk::Box {
      set_hexpand: true,
      set_vexpand: true,
      gtk::ScrolledWindow {
        set_vexpand: true,
        set_hexpand: true,
        set_propagate_natural_width: true,
        #[local_ref]
        topics_view -> gtk::ColumnView {
          set_vexpand: true,
          set_hexpand: true,
          set_show_row_separators: true,
          set_show_column_separators: true,
        }
      }
    }
  }
  
  fn init(
    current: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    // Initialize the ListView wrapper
    let mut view_wrapper = TypedColumnView::<MessageListItem, gtk::SingleSelection>::new();
    view_wrapper.append_column::<OfssetColumn>();
    view_wrapper.append_column::<ValueColumn>();
    
    let model = MessagesPageModel {
      current: current,
      messages_wrapper: view_wrapper,
    };
    
    let topics_view = &model.messages_wrapper.view;
    topics_view.connect_activate(move |_view, idx| {
      let snd = sender.clone();
      //snd.input(MessagesPageMsg::OpenTopic(idx));
    });
    
    let widgets = view_output!();
    ComponentParts { model, widgets }
  }
  
  fn update_with_view(
    &mut self,
    widgets: &mut Self::Widgets,
    msg: MessagesPageMsg,
    sender: ComponentSender<Self>,
    _: &Self::Root,
  ) {
    info!("received message: {:?}", msg);
    
    match msg {
      MessagesPageMsg::List(messages) => {
        for message in messages {
          self.messages_wrapper.append(MessageListItem::new(message));
        }
      }
      
    };
    
    self.update_view(widgets, sender);
  }
}
