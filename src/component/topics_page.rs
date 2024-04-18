use crate::{
    backend::{
        kafka::KafkaBackend,
        repository::{KrustConnection, KrustTopic},
    },
    component::status_bar::{StatusBarMsg, STATUS_BROKER},
};
use gtk::prelude::*;
use relm4::{
    typed_view::column::{LabelColumn, TypedColumnView},
    *,
};

// Table: start
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TopicListItem {
    name: String,
    partition_count: usize,
}

impl TopicListItem {
    fn new(value: KrustTopic) -> Self {
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
    pub topics_wrapper: TypedColumnView<TopicListItem, gtk::SingleSelection>,
    pub is_loading: bool,
    pub search_text: String,
}

#[derive(Debug)]
pub enum TopicsPageMsg {
    List(KrustConnection),
    OpenTopic(u32),
    Search(String),
}

#[derive(Debug)]
pub enum TopicsPageOutput {
    OpenMessagesPage(KrustConnection, KrustTopic),
}

#[derive(Debug)]
pub enum CommandMsg {
    // Data(Vec<KrustMessage>),
    ListFinished(Vec<KrustTopic>),
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
        set_orientation: gtk::Orientation::Vertical,
        set_hexpand: true,
        set_vexpand: true,
        gtk::CenterBox {
          set_orientation: gtk::Orientation::Horizontal,
          set_margin_all: 10,
          set_hexpand: true,
          #[wrap(Some)]
          set_start_widget = &gtk::Box {
            #[name(topics_search_entry)]
            gtk::SearchEntry {
              connect_search_changed[sender] => move |entry| {
                sender.clone().input(TopicsPageMsg::Search(entry.text().to_string()));
              }
            },
          },
        },
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
            current,
            topics_wrapper: view_wrapper,
            is_loading: false,
            search_text: String::default(),
        };

        let topics_view = &model.topics_wrapper.view;
        let snd = sender.clone();
        topics_view.connect_activate(move |_view, idx| {
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
        match msg {
            TopicsPageMsg::Search(term) => {
                self.topics_wrapper.clear_filters();
                let search_term = term.clone();
                self.topics_wrapper
                    .add_filter(move |item| item.name.contains(search_term.as_str()));
            }
            TopicsPageMsg::List(conn) => {
                STATUS_BROKER.send(StatusBarMsg::Start);
                self.topics_wrapper.clear();
                self.current = Some(conn.clone());
                sender.oneshot_command(async move {
                    let kafka = KafkaBackend::new(&conn);
                    let topics = kafka.list_topics().await;

                    CommandMsg::ListFinished(topics)
                });
            }
            TopicsPageMsg::OpenTopic(idx) => {
                let item = self.topics_wrapper.get_visible(idx).unwrap();
                let conn_id = self.current.as_ref().and_then(|c| c.id);
                let connection = self.current.clone();
                let topic = KrustTopic {
                    connection_id: conn_id,
                    name: item.borrow().name.clone(),
                    cached: None,
                    partitions: vec![],
                };
                sender
                    .output(TopicsPageOutput::OpenMessagesPage(
                        connection.unwrap(),
                        topic,
                    ))
                    .unwrap();
            }
        };

        self.update_view(widgets, sender);
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match message {
            CommandMsg::ListFinished(topics) => {
                self.topics_wrapper.clear();
                for topic in topics.into_iter().filter(|t| !t.name.starts_with("__")) {
                    self.topics_wrapper
                        .insert_sorted(TopicListItem::new(topic), |a, b| a.cmp(b));
                }
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some("Topics loaded!".into()),
                });
            }
        }
    }
}
