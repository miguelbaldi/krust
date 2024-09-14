use std::{cell::RefCell, cmp::Ordering, collections::HashMap};

use crate::modals::utils::build_confirmation_alert;
use crate::AppMsg;
use crate::{
    backend::{
        kafka::KafkaBackend,
        repository::{KrustConnection, KrustTopic},
    },
    component::status_bar::{StatusBarMsg, STATUS_BROKER},
    config::ExternalError,
    modals::utils::show_error_alert,
    Repository, TOASTER_BROKER,
};
use adw::{prelude::*, AlertDialog};
use gtk::glib::SignalHandlerId;
use relm4::{
    factory::{DynamicIndex, FactoryComponent},
    typed_view::column::{LabelColumn, RelmColumn, TypedColumnView},
    *,
};
use tracing::{debug, error, info};
use uuid::Uuid;

use super::create_dialog::{CreateTopicDialogModel, CreateTopicDialogMsg, CreateTopicDialogOutput};

relm4::new_action_group!(pub(super) TopicListActionGroup, "topic-list");
relm4::new_stateless_action!(pub(super) FavouriteAction, TopicListActionGroup, "toggle-favourite");

// Table: start
#[derive(Debug)]
pub struct TopicListItem {
    name: String,
    partition_count: usize,
    favourite: bool,
    sender: FactorySender<TopicsTabModel>,
    clicked_handler_id: RefCell<Option<SignalHandlerId>>,
}

impl TopicListItem {
    fn new(value: KrustTopic, sender: FactorySender<TopicsTabModel>) -> Self {
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
        //self.partial_cmp(other).unwrap()
        match PartialOrd::partial_cmp(&self.favourite, &other.favourite) {
            Some(Ordering::Equal) => match PartialOrd::partial_cmp(&self.name, &other.name) {
                Some(Ordering::Equal) => {
                    PartialOrd::partial_cmp(&self.partition_count, &other.partition_count).unwrap()
                }
                cmp => cmp.unwrap(),
            },
            Some(Ordering::Less) => Ordering::Greater,
            Some(Ordering::Greater) => Ordering::Less,
            cmp => cmp.unwrap(),
        }
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
        Some(self.cmp(other))
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
        let button = gtk::CheckButton::builder()
            .css_name("btn-favourite")
            .build();
        (button, ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, button: &mut Self::Root) {
        button.set_active(item.favourite);
        let topic_name = item.name.clone();
        let sender = item.sender.clone();
        let signal_id = button.connect_toggled(move |b| {
            info!("FavouriteColumn[{}][{}]", &topic_name, b.is_active());
            sender.input(TopicsTabMsg::FavouriteToggled {
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

pub struct TopicsTabInit {
    pub connection: KrustConnection,
}

#[derive(Debug)]
pub struct TopicsTabModel {
    pub current: Option<KrustConnection>,
    pub topics_wrapper: TypedColumnView<TopicListItem, gtk::SingleSelection>,
    pub is_loading: bool,
    pub search_text: String,
    pub create_topic: Controller<CreateTopicDialogModel>,
    pub confirmation_alert: AlertDialog,
    pub selected_topic_name: Option<String>,
}

#[derive(Debug)]
pub enum TopicsTabMsg {
    List(KrustConnection),
    OpenTopic(u32),
    Search(String),
    FavouriteToggled { topic_name: String, is_active: bool },
    ToggleFavouritesFilter(bool),
    RefreshTopics,
    CreateTopic,
    DeleteTopic,
    ConfirmDeleteTopic,
    Ignore,
    SelectTopic(u32),
}

#[derive(Debug)]
pub enum TopicsTabOutput {
    OpenMessagesPage(KrustConnection, KrustTopic),
    HandleError(KrustConnection, bool),
}

#[derive(Debug)]
pub enum CommandMsg {
    ListFinished(Vec<KrustTopic>),
    ShowError(ExternalError),
    DeleteTopicResult,
}

impl TopicsTabModel {
    fn fetch_persited_topics(&self) -> Result<HashMap<String, KrustTopic>, ExternalError> {
        let result = if let Some(conn) = self.current.clone() {
            let mut repo = Repository::new();
            let topics = repo.find_topics_by_connection(conn.id.unwrap())?;
            debug!("fetch_persited_topics::{:?}", topics);
            topics
                .into_iter()
                .map(|t| (t.name.clone(), t.clone()))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        Ok(result)
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for TopicsTabModel {
    type Init = TopicsTabInit;
    type Input = TopicsTabMsg;
    type Output = TopicsTabOutput;
    type CommandOutput = CommandMsg;
    type ParentWidget = adw::TabView;

    view! {
        #[root]
        #[name(root)]
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
                        set_width_chars: 50,
                        connect_search_changed[sender] => move |entry| {
                            sender.clone().input(TopicsTabMsg::Search(entry.text().to_string()));
                        },
                    },
                    #[name(btn_cache_toggle)]
                    gtk::ToggleButton {
                        set_margin_start: 5,
                        set_label: "Favourites",
                        add_css_class: "krust-toggle",
                        connect_toggled[sender] => move |btn| {
                            sender.input(TopicsTabMsg::ToggleFavouritesFilter(btn.is_active()));
                        },
                    },
                },
                #[wrap(Some)]
                set_end_widget = &gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    #[name(btn_create_topic)]
                    gtk::Button {
                        set_tooltip_text: Some("Create topic"),
                        set_icon_name: "list-add-symbolic",
                        set_margin_start: 5,
                        connect_clicked[sender] => move |_| {
                            info!("Create topic");
                            sender.input(TopicsTabMsg::CreateTopic);
                        },
                    },
                    #[name(btn_delete_topic)]
                    gtk::Button {
                        set_tooltip_text: Some("Delete selected topic"),
                        set_icon_name: "edit-delete-symbolic",
                        set_margin_start: 5,
                        add_css_class: "krust-destroy",
                        connect_clicked[sender] => move |_| {
                            sender.input(TopicsTabMsg::DeleteTopic);
                        },
                    },
                    #[name(btn_refresh)]
                    gtk::Button {
                        set_icon_name: "media-playlist-repeat-symbolic",
                        set_margin_start: 5,
                        connect_clicked[sender] => move |_| {
                            sender.input(TopicsTabMsg::RefreshTopics);
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
                self.topics_wrapper.view.clone() -> gtk::ColumnView {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_show_row_separators: true,
                }
            }
        }
    }

    fn init_model(current: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        // Initialize the ListView wrapper
        let mut view_wrapper = TypedColumnView::<TopicListItem, gtk::SingleSelection>::new();
        view_wrapper.append_column::<FavouriteColumn>();
        view_wrapper.append_column::<NameColumn>();
        view_wrapper.append_column::<PartitionCountColumn>();

        // Add a filter and disable it
        view_wrapper.add_filter(|item| item.favourite);
        view_wrapper.set_filter_status(0, false);
        let connection = current.connection.clone();

        let create_topic = CreateTopicDialogModel::builder()
            .launch(Some(connection.clone()))
            .forward(sender.input_sender(), |msg| match msg {
                CreateTopicDialogOutput::RefreshTopics => TopicsTabMsg::RefreshTopics,
            });

        let confirmation_alert = build_confirmation_alert(
            "Delete".to_string(),
            "Are you sure you want to delete the topic?".to_string(),
        );
        let snd: FactorySender<TopicsTabModel> = sender.clone();
        confirmation_alert.connect_response(Some("cancel"), move |_, _| {
            snd.input(TopicsTabMsg::Ignore);
        });
        let snd: FactorySender<TopicsTabModel> = sender.clone();
        confirmation_alert.connect_response(Some("confirm"), move |_, _| {
            snd.input(TopicsTabMsg::ConfirmDeleteTopic);
        });
        let model = TopicsTabModel {
            current: Some(connection),
            topics_wrapper: view_wrapper,
            is_loading: false,
            search_text: String::default(),
            create_topic,
            confirmation_alert,
            selected_topic_name: None,
        };

        let topics_view = &model.topics_wrapper.view;
        let snd: FactorySender<TopicsTabModel> = sender.clone();
        topics_view.connect_activate(move |_view, idx| {
            snd.input(TopicsTabMsg::OpenTopic(idx));
        });
        let snd: FactorySender<TopicsTabModel> = sender.clone();
        topics_view
            .model()
            .unwrap()
            .connect_selection_changed(move |selection_model, _i, _j| {
                let size = selection_model.selection().size();
                if size == 1 {
                    let selected = selection_model.selection().minimum();
                    info!("messages_view::selection_changed[{}]", selected);
                    snd.input(TopicsTabMsg::SelectTopic(selected));
                }
            });
        sender.input(TopicsTabMsg::RefreshTopics);
        model
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: TopicsTabMsg,
        sender: FactorySender<Self>,
    ) {
        match msg {
            TopicsTabMsg::Ignore => {}
            TopicsTabMsg::SelectTopic(idx) => {
                let item = self.topics_wrapper.get_visible(idx).unwrap();
                let topic_name = item.borrow().name.clone();
                self.selected_topic_name = Some(topic_name);
            }
            TopicsTabMsg::RefreshTopics => {
                if let Some(connection) = self.current.clone() {
                    sender.input(TopicsTabMsg::List(connection));
                }
            }
            TopicsTabMsg::CreateTopic => {
                self.create_topic.emit(CreateTopicDialogMsg::Show);
            }
            TopicsTabMsg::ConfirmDeleteTopic => {
                info!("deleting topic {:?}", self.selected_topic_name.clone());
                let connection = self.current.clone().unwrap();
                if let Some(topic_name) = self.selected_topic_name.clone() {
                    sender.oneshot_command(async move {
                        let kafka = KafkaBackend::new(&connection);
                        let result = kafka.delete_topic(topic_name).await;
                        match result {
                            Err(e) => CommandMsg::ShowError(e),
                            Ok(_) => CommandMsg::DeleteTopicResult,
                        }
                    })
                }
            }
            TopicsTabMsg::DeleteTopic => {
                self.confirmation_alert.present(&widgets.root);
            }
            TopicsTabMsg::Search(term) => {
                self.topics_wrapper.clear_filters();
                let search_term = term.clone();
                self.topics_wrapper
                    .add_filter(move |item| item.name.contains(search_term.as_str()));
            }
            TopicsTabMsg::List(conn) => {
                STATUS_BROKER.send(StatusBarMsg::Start);
                let id = Uuid::new_v4();
                TOASTER_BROKER.send(AppMsg::ShowToast(
                    id.to_string(),
                    "Connecting...".to_string(),
                ));
                self.topics_wrapper.clear();
                self.current = Some(conn.clone());
                let result_topics_map = self.fetch_persited_topics();
                let output = sender.output_sender().clone();
                match result_topics_map {
                    Ok(topics_map) => {
                        sender.oneshot_command(async move {
                            let kafka = KafkaBackend::new(&conn);
                            info!("Trying to list topics...");
                            let topics_result = kafka.list_topics().await;
                            match topics_result {
                                Ok(mut topics) => {
                                    for topic in topics.iter_mut() {
                                        if let Some(t) = topics_map.get(&topic.name) {
                                            debug!("found topic: {:?}", t);
                                            topic.favourite = t.favourite;
                                            topic.cached = t.cached;
                                        }
                                    }
                                    TOASTER_BROKER.send(AppMsg::HideToast(id.to_string()));
                                    CommandMsg::ListFinished(topics)
                                }
                                Err(error) => {
                                    TOASTER_BROKER.send(AppMsg::HideToast(id.to_string()));
                                    output.emit(TopicsTabOutput::HandleError(conn.clone(), true));
                                    let display_error = if let ExternalError::KafkaUnexpectedError(
                                        rdkafka::error::KafkaError::MetadataFetch(_),
                                    ) = &error
                                    {
                                        ExternalError::DisplayError(
                                            "connecting".to_string(),
                                            "kafka broker unreachable".to_string(),
                                        )
                                    } else {
                                        error
                                    };
                                    CommandMsg::ShowError(display_error)
                                }
                            }
                        });
                    }
                    Err(err) => {
                        TOASTER_BROKER.send(AppMsg::HideToast(id.to_string()));
                        sender
                            .output_sender()
                            .emit(TopicsTabOutput::HandleError(conn.clone(), true));
                        sender.command_sender().emit(CommandMsg::ShowError(err));
                    }
                };
            }
            TopicsTabMsg::OpenTopic(idx) => {
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
                    .output(TopicsTabOutput::OpenMessagesPage(
                        connection.unwrap(),
                        topic,
                    ))
                    .unwrap();
            }
            TopicsTabMsg::FavouriteToggled {
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
                //sender.input(TopicsTabMsg::List(self.current.clone().unwrap()));
            }
            TopicsTabMsg::ToggleFavouritesFilter(is_active) => {
                if is_active {
                    self.topics_wrapper.clear_filters();
                    self.topics_wrapper.add_filter(|item| item.favourite);
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
        sender: FactorySender<Self>,
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
            CommandMsg::ShowError(error) => {
                let error_message = format!("{}", error);
                error!(error_message);
                show_error_alert(&widgets.root, error_message);
            }
            CommandMsg::DeleteTopicResult => {
                sender.input(TopicsTabMsg::RefreshTopics);
            }
        }
    }
}
