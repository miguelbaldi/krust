use std::{cell::RefCell, cmp::Ordering, collections::HashMap};

use crate::{
    backend::{
        kafka::KafkaBackend,
        repository::{KrustConnection, KrustTopic},
    },
    component::status_bar::{StatusBarMsg, STATUS_BROKER},
    config::ExternalError,
    Repository,
};
use gtk::{glib::SignalHandlerId, prelude::*};
use relm4::{
    typed_view::column::{LabelColumn, RelmColumn, TypedColumnView},
    *,
};
use tracing::{debug, info};

relm4::new_action_group!(pub(super) TopicListActionGroup, "topic-list");
relm4::new_stateless_action!(pub(super) FavouriteAction, TopicListActionGroup, "toggle-favourite");

// Table: start
#[derive(Debug)]
pub struct TopicListItem {
    name: String,
    partition_count: usize,
    favourite: bool,
    sender: ComponentSender<TopicsPageModel>,
    clicked_handler_id: RefCell<Option<SignalHandlerId>>,
}

impl TopicListItem {
    fn new(value: KrustTopic, sender: ComponentSender<TopicsPageModel>) -> Self {
        Self {
            name: value.name,
            partition_count: value.partitions.len(),
            favourite: value.favourite.unwrap_or(false),
            sender,
            clicked_handler_id: RefCell::new(None),
        }
    }
}

impl Eq for TopicListItem {}

impl Ord for TopicListItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq for TopicListItem {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.partition_count == other.partition_count
            && self.favourite == other.favourite
    }
}

impl PartialOrd for TopicListItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match PartialOrd::partial_cmp(&self.favourite, &other.favourite) {
            Some(Ordering::Equal) => match PartialOrd::partial_cmp(&self.name, &other.name) {
                Some(Ordering::Equal) => {
                    PartialOrd::partial_cmp(&self.partition_count, &other.partition_count)
                }
                cmp => cmp,
            },
            Some(Ordering::Less) => Some(Ordering::Greater),
            Some(Ordering::Greater) => Some(Ordering::Less),
            cmp => cmp,
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

struct FavouriteColumn;

impl RelmColumn for FavouriteColumn {
    type Root = gtk::CheckButton;
    type Widgets = ();
    type Item = TopicListItem;

    const COLUMN_NAME: &'static str = "Favourite";
    const ENABLE_RESIZE: bool = false;
    const ENABLE_EXPAND: bool = false;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        let button = gtk::CheckButton::builder().css_name("btn-favourite").build();
        (button, ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, button: &mut Self::Root) {
        button.set_active(item.favourite);
        let topic_name = item.name.clone();
        let sender = item.sender.clone();
        let signal_id = button.connect_toggled(move |b| {
            info!("FavouriteColumn[{}][{}]", &topic_name, b.is_active());
            sender.input(TopicsPageMsg::FavouriteToggled {
                topic_name: topic_name.clone(),
                is_active: b.is_active(),
            });
        });
        item.clicked_handler_id = RefCell::new(Some(signal_id));
    }
    fn unbind(item: &mut Self::Item, _: &mut Self::Widgets, button: &mut Self::Root) {
        if let Some(id) = item.clicked_handler_id.take() {
            button.disconnect(id);
        };
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
    FavouriteToggled { topic_name: String, is_active: bool },
    ToggleFavouritesFilter(bool),
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

impl TopicsPageModel {
    fn fetch_persited_topics(&self) -> Result<HashMap<String, KrustTopic>, ExternalError> {
        let result = if let Some(conn) = self.current.clone() {
            let mut repo = Repository::new();
            let topics = repo.find_topics_by_connection(conn.id.unwrap())?;
            debug!("fetch_persited_topics::{:?}", topics);
            let topics_map = topics
                .into_iter()
                .map(|t| (t.name.clone(), t.clone()))
                .collect::<HashMap<_, _>>();
            topics_map
        } else {
            HashMap::new()
        };
        Ok(result)
    }
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
                    set_orientation: gtk::Orientation::Horizontal,
                    #[name(topics_search_entry)]
                    gtk::SearchEntry {
                        connect_search_changed[sender] => move |entry| {
                            sender.clone().input(TopicsPageMsg::Search(entry.text().to_string()));
                        },
                    },
                    #[name(btn_cache_toggle)]
                    gtk::ToggleButton {
                        set_margin_start: 5,
                        set_label: "Favourites",
                        add_css_class: "krust-toggle",
                        connect_toggled[sender] => move |btn| {
                            sender.input(TopicsPageMsg::ToggleFavouritesFilter(btn.is_active()));
                        },
                    },
                },
            },
            #[name(topics_scrolled_windows)]
            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_propagate_natural_width: true,
                set_vscrollbar_policy: gtk::PolicyType::Always,
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
        view_wrapper.append_column::<FavouriteColumn>();
        view_wrapper.append_column::<NameColumn>();
        view_wrapper.append_column::<PartitionCountColumn>();

         // Add a filter and disable it
         view_wrapper.add_filter(|item| item.favourite );
         view_wrapper.set_filter_status(0, false);

        let model = TopicsPageModel {
            current,
            topics_wrapper: view_wrapper,
            is_loading: false,
            search_text: String::default(),
        };

        let topics_view = &model.topics_wrapper.view;
        let snd: ComponentSender<TopicsPageModel> = sender.clone();
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
                let topics_map = self.fetch_persited_topics().unwrap();

                sender.oneshot_command(async move {
                    let kafka = KafkaBackend::new(&conn);
                    let mut topics = kafka.list_topics().await;
                    for topic in topics.iter_mut() {
                        if let Some(t) = topics_map.get(&topic.name) {
                            debug!("found topic: {:?}", t);
                            topic.favourite = t.favourite;
                            topic.cached = t.cached;
                        }
                    }
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
                    total: None,
                    favourite: None,
                };
                sender
                    .output(TopicsPageOutput::OpenMessagesPage(
                        connection.unwrap(),
                        topic,
                    ))
                    .unwrap();
            }
            TopicsPageMsg::FavouriteToggled {
                topic_name,
                is_active,
            } => {
                info!("topic {} favourite toggled {}", topic_name, is_active);
                let conn_id = self.current.clone().unwrap().id.unwrap();
                let mut repo = Repository::new();
                let mut topic = repo.find_topic(conn_id, &topic_name);
                info!("persisted topic::{:?}", &topic);
                if let Some(topic) = topic.as_mut() {
                    topic.favourite = Some(is_active);
                    repo.save_topic(conn_id, topic).unwrap();
                } else if is_active {
                    let topic = KrustTopic {
                        connection_id: Some(conn_id),
                        name: topic_name.clone(),
                        cached: None,
                        partitions: vec![],
                        total: None,
                        favourite: Some(is_active),
                    };
                    repo.save_topic(conn_id, &topic).unwrap();
                }
                //sender.input(TopicsPageMsg::List(self.current.clone().unwrap()));
            }
            TopicsPageMsg::ToggleFavouritesFilter(is_active) => {
                if is_active {
                    self.topics_wrapper.clear_filters();
                    self.topics_wrapper.add_filter(|item| item.favourite );
                } else {
                    self.topics_wrapper.clear_filters();
                }
            }
        };

        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match message {
            CommandMsg::ListFinished(topics) => {
                self.topics_wrapper.clear();
                for topic in topics.into_iter().filter(|t| !t.name.starts_with("__")) {
                    let snd = sender.clone();
                    self.topics_wrapper
                        .insert_sorted(TopicListItem::new(topic, snd), |a, b| a.cmp(b));
                }
                let vadj = widgets.topics_scrolled_windows.vadjustment();
                info!(
                    "vertical scroll adjustment: upper={}, lower={}, page_size={}",
                    vadj.upper(),
                    vadj.lower(),
                    vadj.page_size()
                );
                let scroll_result = widgets
                    .topics_scrolled_windows
                    .emit_scroll_child(gtk::ScrollType::Start, false);
                let vadj = widgets.topics_scrolled_windows.vadjustment();
                info!(
                    "vertical scroll adjustment after: upper={}, lower={}, page_size={}::{}",
                    vadj.upper(),
                    vadj.lower(),
                    vadj.page_size(),
                    scroll_result
                );
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some("Topics loaded!".into()),
                });
            }
        }
    }
}
