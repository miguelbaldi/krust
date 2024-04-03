use gtk::prelude::*;
use relm4::{
  typed_view::column::{LabelColumn, TypedColumnView},
  *,
};
use tracing::info;

use crate::backend::{kafka::{KafkaBackend, Topic}, repository::{KrustConnection, KrustMessage}};

// Table: start
#[derive(Debug, PartialEq, Eq)]
struct TopicListItem {
  name: String,
  partition_count: usize, 
}

impl TopicListItem {
  fn new(value: Topic) -> Self {
    Self {
      name: value.name,
      partition_count: value.partitions.len(),
    }
  }
}

struct PartitionCountColumn;

impl LabelColumn for PartitionCountColumn {
  type Item = TopicListItem;
  type Value = usize;
  
  const COLUMN_NAME: &'static str = "Partitions";
  
  const ENABLE_SORT: bool = true;
  const ENABLE_RESIZE: bool = true;
  
  fn get_cell_value(item: &Self::Item) -> Self::Value {
    item.partition_count
  }
  
  fn format_cell_value(value: &Self::Value) -> String {
    format!("{}", value)
  }
}

struct NameColumn;

impl LabelColumn for NameColumn {
  type Item = TopicListItem;
  type Value = String;
  
  const COLUMN_NAME: &'static str = "Name";
  const ENABLE_RESIZE: bool = true;
  const ENABLE_EXPAND: bool = true;
  const ENABLE_SORT: bool = true;
  
  fn get_cell_value(item: &Self::Item) -> Self::Value {
    item.name.clone()
  }
  
  fn format_cell_value(value: &Self::Value) -> String {
    value.clone()
  }
}

// Table: end

#[derive(Debug)]
pub struct TopicsPageModel {
  pub current: Option<KrustConnection>,
  topics_wrapper: TypedColumnView<TopicListItem, gtk::SingleSelection>,
}

#[derive(Debug)]
pub enum TopicsPageMsg {
  List(KrustConnection),
  OpenTopic(u32),
}

#[derive(Debug)]
pub enum TopicsPageOutput {
  OpenMessagesPage(Vec<KrustMessage>),
}

#[derive(Debug)]
pub enum CommandMsg {
    Data(Vec<KrustMessage>),
}

#[relm4::component(pub)]
impl Component for TopicsPageModel {
  type Init = Option<KrustConnection>;
  type Input = TopicsPageMsg;
  type Output = TopicsPageOutput;
  type CommandOutput = CommandMsg;
  
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
    let mut view_wrapper = TypedColumnView::<TopicListItem, gtk::SingleSelection>::new();
    view_wrapper.append_column::<NameColumn>();
    view_wrapper.append_column::<PartitionCountColumn>();
    
    let model = TopicsPageModel {
      current: current,
      topics_wrapper: view_wrapper,
    };
    
    let topics_view = &model.topics_wrapper.view;
    topics_view.connect_activate(move |_view, idx| {
      let snd = sender.clone();
      snd.input(TopicsPageMsg::OpenTopic(idx));
    });
    
    let widgets = view_output!();
    ComponentParts { model, widgets }
  }
  
  fn update_with_view(
    &mut self,
    widgets: &mut Self::Widgets,
    msg: TopicsPageMsg,
    sender: ComponentSender<Self>,
    _: &Self::Root,
  ) {
    info!("received message: {:?}", msg);
    
    match msg {
      TopicsPageMsg::List(conn) => {
        self.current = Some(conn.clone());
        let kafka = KafkaBackend::new(conn);
        let topics = kafka.list_topics();
        for topic in topics {
          self.topics_wrapper.append(TopicListItem::new(topic));
        }
      }
      TopicsPageMsg::OpenTopic(idx) => {
        let item = self.topics_wrapper.get_visible(idx).unwrap();
        let topic_name = item.borrow().name.clone();
        let conn = self.current.clone().unwrap();
        sender.oneshot_command(async {
          let kafka = KafkaBackend::new(conn);
          let message_count = kafka.topic_message_count(topic_name.clone());
          info!("selected topic {} with {} messages", topic_name.clone(), message_count);
          // Run async background task
          let messages = kafka.list_messages_for_topic(topic_name).await;
          for message in messages.clone() {
            info!("MESSAGE::{:?}", message);
          }
          CommandMsg::Data(messages)
        });
        
      }
    };
    
    self.update_view(widgets, sender);
  }

  fn update_cmd(
    &mut self,
    message: Self::CommandOutput,
    sender: ComponentSender<Self>,
    _: &Self::Root,
) {
    match message {
        CommandMsg::Data(data) => {
          sender.output(TopicsPageOutput::OpenMessagesPage(data)).unwrap();
        }
    }
}
}
