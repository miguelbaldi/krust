#![allow(deprecated)]
// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use chrono::{TimeZone, Utc};
use chrono_tz::America;
use gtk::{gdk::DisplayManager, ColumnViewSorter};
use relm4::{typed_view::column::TypedColumnView, *};
use relm4_components::simple_combo_box::SimpleComboBox;
use sourceview::prelude::*;
use sourceview5 as sourceview;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace};

use crate::{
    backend::{
        repository::{KrustConnection, KrustMessage, KrustTopic},
        worker::{
            MessagesCleanupRequest, MessagesMode, MessagesRequest, MessagesResponse,
            MessagesWorker, PageOp,
        },
    },
    component::{
        messages::lists::{
            HeaderListItem, HeaderNameColumn, HeaderValueColumn, MessageListItem,
            MessageOfssetColumn, MessagePartitionColumn, MessageTimestampColumn,
            MessageValueColumn,
        },
        status_bar::{StatusBarMsg, STATUS_BROKER},
    },
    Repository, DATE_TIME_FORMAT,
};

pub static LIVE_MESSAGES_BROKER: MessageBroker<MessagesPageMsg> = MessageBroker::new();

#[derive(Debug)]
pub struct MessagesPageModel {
    token: CancellationToken,
    topic: Option<KrustTopic>,
    mode: MessagesMode,
    connection: Option<KrustConnection>,
    messages_wrapper: TypedColumnView<MessageListItem, gtk::MultiSelection>,
    headers_wrapper: TypedColumnView<HeaderListItem, gtk::NoSelection>,
    page_size_combo: Controller<SimpleComboBox<u16>>,
    page_size: u16,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
    Open(KrustConnection, KrustTopic),
    GetMessages,
    GetNextMessages,
    GetPreviousMessages,
    StopGetMessages,
    RefreshMessages,
    UpdateMessages(MessagesResponse),
    UpdateMessage(KrustMessage),
    OpenMessage(u32),
    Selection(u32),
    PageSizeChanged(usize),
    ToggleMode(bool),
}

#[derive(Debug)]
pub enum CommandMsg {
    Data(MessagesResponse),
}

const AVAILABLE_PAGE_SIZES: [u16; 4] = [50, 100, 500, 1000];

#[relm4::component(pub)]
impl Component for MessagesPageModel {
    type Init = ();
    type Input = MessagesPageMsg;
    type Output = ();
    type CommandOutput = CommandMsg;

    view! {
        #[root]
        gtk::Paned {
            set_orientation: gtk::Orientation::Vertical,
            //set_resize_start_child: true,
            #[wrap(Some)]
            set_start_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,
                set_vexpand: true,
                gtk::CenterBox {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Fill,
                    set_margin_all: 10,
                    set_hexpand: true,
                    #[wrap(Some)]
                    set_start_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        #[name(btn_get_messages)]
                        gtk::Button {
                            set_icon_name: "media-playback-start-symbolic",
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesPageMsg::GetMessages);
                            },
                        },
                        #[name(btn_stop_messages)]
                        gtk::Button {
                            set_icon_name: "media-playback-stop-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesPageMsg::StopGetMessages);
                            },
                        },
                        #[name(btn_cache_refresh)]
                        gtk::Button {
                            set_icon_name: "media-playlist-repeat-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesPageMsg::RefreshMessages);
                            },
                        },
                        #[name(btn_cache_toggle)]
                        gtk::ToggleButton {
                            set_margin_start: 5,
                            set_label: "Cache",
                            add_css_class: "krust-toggle",
                            connect_toggled[sender] => move |btn| {
                                sender.input(MessagesPageMsg::ToggleMode(btn.is_active()));
                            },
                        },
                        #[name(cache_timestamp)]
                        gtk::Label {
                            set_margin_start: 5,
                            set_label: "",
                            set_visible: false,
                        }
                    },
                    #[wrap(Some)]
                    set_end_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Fill,
                        set_hexpand: true,
                        #[name(topics_search_entry)]
                        gtk::SearchEntry {
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,

                        },
                        gtk::Button {
                            set_icon_name: "edit-find-symbolic",
                            set_margin_start: 5,
                        },
                    },
                },
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_propagate_natural_width: true,
                    #[local_ref]
                    messages_view -> gtk::ColumnView {
                        set_vexpand: true,
                        set_hexpand: true,
                        set_show_row_separators: true,
                        set_show_column_separators: true,
                        set_single_click_activate: false,
                        set_enable_rubberband: true,
                    }
                },
            },
            #[wrap(Some)]
            set_end_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                append = &gtk::StackSwitcher {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_stack: Some(&message_viewer),
                },
                append: message_viewer = &gtk::Stack {
                    add_child = &gtk::Box {
                        set_hexpand: true,
                        set_vexpand: true,
                        #[name = "value_container"]
                        gtk::ScrolledWindow {
                            add_css_class: "bordered",
                            set_vexpand: true,
                            set_hexpand: true,
                            set_propagate_natural_height: true,
                            set_overflow: gtk::Overflow::Hidden,
                            set_valign: gtk::Align::Fill,
                            #[name = "value_source_view"]
                            sourceview::View {
                                add_css_class: "file-preview-source",
                                set_cursor_visible: true,
                                set_editable: false,
                                set_monospace: true,
                                set_show_line_numbers: true,
                                set_valign: gtk::Align::Fill,
                            }
                        },
                    } -> {
                        set_title: "Value",
                        set_name: "Value",
                    },
                    add_child = &gtk::Box {
                        gtk::ScrolledWindow {
                            set_vexpand: true,
                            set_hexpand: true,
                            set_propagate_natural_width: true,
                            #[local_ref]
                            headers_view -> gtk::ColumnView {
                                set_vexpand: true,
                                set_hexpand: true,
                                set_show_row_separators: true,
                                set_show_column_separators: true,
                            }
                        },
                    } -> {
                        set_title: "Header",
                        set_name: "Header",
                    },
                },
                gtk::CenterBox {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_halign: gtk::Align::Fill,
                    set_margin_all: 10,
                    set_hexpand: true,
                    #[wrap(Some)]
                    set_start_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        gtk::Label {
                            set_label: "Total"
                        },
                        #[name(pag_total_entry)]
                        gtk::Entry {
                            set_editable: false,
                            set_sensitive: false,
                            set_margin_start: 5,
                            set_width_chars: 10,
                        },
                    },
                    #[wrap(Some)]
                    set_center_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Center,
                        set_hexpand: true,
                        #[name(cached_centered_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            #[name(pag_current_entry)]
                            gtk::Entry {
                                set_editable: false,
                                set_sensitive: false,
                                set_margin_start: 5,
                                set_width_chars: 10,
                            },
                            gtk::Label {
                                set_label: "of",
                                set_margin_start: 5,
                            },
                            #[name(pag_last_entry)]
                            gtk::Entry {
                                set_editable: false,
                                set_sensitive: false,
                                set_margin_start: 5,
                                set_width_chars: 10,
                            },
                        },

                    },
                    #[wrap(Some)]
                    set_end_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        #[name(cached_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            #[name(first_offset)]
                            gtk::Label {
                                set_label: "",
                                set_visible: false,
                            },
                            #[name(first_partition)]
                            gtk::Label {
                                set_label: "",
                                set_visible: false,
                            },
                            #[name(last_offset)]
                            gtk::Label {
                                set_label: "",
                                set_visible: false,
                            },
                            #[name(last_partition)]
                            gtk::Label {
                                set_label: "",
                                set_visible: false,
                            },
                            gtk::Label {
                                set_label: "Page size",
                                set_margin_start: 5,
                            },
                            model.page_size_combo.widget() -> &gtk::ComboBoxText {
                                set_margin_start: 5,
                            },
                            #[name(btn_previous_page)]
                            gtk::Button {
                                set_margin_start: 5,
                                set_icon_name: "go-previous",
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesPageMsg::GetPreviousMessages);
                                },
                            },
                            #[name(btn_next_page)]
                            gtk::Button {
                                set_margin_start: 5,
                                set_icon_name: "go-next",
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesPageMsg::GetNextMessages);
                                },
                            },
                        },
                        #[name(live_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            gtk::Label {
                                set_label: "Max messages",
                                set_margin_start: 5,
                            },
                            #[name(max_messages)]
                            gtk::Entry {
                                set_margin_start: 5,
                                set_width_chars: 10,
                            },
                        },
                    },
                },
            },
        }

    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize the messages ListView wrapper
        let mut messages_wrapper = TypedColumnView::<MessageListItem, gtk::MultiSelection>::new();
        messages_wrapper.append_column::<MessagePartitionColumn>();
        messages_wrapper.append_column::<MessageOfssetColumn>();
        messages_wrapper.append_column::<MessageValueColumn>();
        messages_wrapper.append_column::<MessageTimestampColumn>();
        // Initialize the headers ListView wrapper
        let mut headers_wrapper = TypedColumnView::<HeaderListItem, gtk::NoSelection>::new();
        headers_wrapper.append_column::<HeaderNameColumn>();
        headers_wrapper.append_column::<HeaderValueColumn>();
        let default_idx = 0;
        let page_size_combo = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: AVAILABLE_PAGE_SIZES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(sender.input_sender(), MessagesPageMsg::PageSizeChanged);
        page_size_combo.widget().queue_allocate();
        let model = MessagesPageModel {
            token: CancellationToken::new(),
            mode: MessagesMode::Live,
            topic: None,
            connection: None,
            messages_wrapper,
            headers_wrapper,
            page_size_combo,
            page_size: AVAILABLE_PAGE_SIZES[0],
        };

        let messages_view = &model.messages_wrapper.view;
        let headers_view = &model.headers_wrapper.view;
        let sender_for_selection = sender.clone();
        messages_view
            .model()
            .unwrap()
            .connect_selection_changed(move |selection_model, _, _| {
                sender_for_selection.input(MessagesPageMsg::Selection(selection_model.n_items()));
            });
        let sender_for_activate = sender.clone();
        messages_view.connect_activate(move |_view, idx| {
            sender_for_activate.input(MessagesPageMsg::OpenMessage(idx));
        });

        messages_view.sorter().unwrap().connect_changed(move |sorter, change| {
            let order = sorter.order();
            let csorter: &ColumnViewSorter = sorter.downcast_ref().unwrap();
            info!("sort order changed: {:?}:{:?}", change, order);
            for i in 0..= csorter.n_sort_columns() {
                let (cvc, sort) = csorter.nth_sort_column(i);
                info!("column[{:?}]sort[{:?}]", cvc.map(|col| { col.title() }), sort);
            }
        });

        let widgets = view_output!();

        let buffer = widgets
            .value_source_view
            .buffer()
            .downcast::<sourceview::Buffer>()
            .expect("sourceview was not backed by sourceview buffer");

        if let Some(scheme) = &sourceview::StyleSchemeManager::new().scheme("oblivion") {
            buffer.set_style_scheme(Some(scheme));
        }
        let language = sourceview::LanguageManager::default().language("json");
        buffer.set_language(language.as_ref());

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: MessagesPageMsg,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match msg {
            MessagesPageMsg::ToggleMode(toggle) => {
                self.mode = if toggle {
                    widgets.cached_controls.set_visible(true);
                    widgets.cached_centered_controls.set_visible(true);
                    widgets.live_controls.set_visible(false);
                    MessagesMode::Cached { refresh: false }
                } else {
                    widgets.live_controls.set_visible(true);
                    widgets.cached_controls.set_visible(false);
                    widgets.cached_centered_controls.set_visible(false);
                    widgets.cache_timestamp.set_visible(false);
                    widgets.cache_timestamp.set_text("");
                    let cloned_topic = self.topic.clone().unwrap();
                    let topic = KrustTopic {
                        name: cloned_topic.name.clone(),
                        connection_id: cloned_topic.connection_id,
                        cached: None,
                        partitions: vec![],
                    };
                    let conn = self.connection.clone().unwrap();
                    MessagesWorker::new().cleanup_messages(&MessagesCleanupRequest {
                        connection: conn,
                        topic: topic.clone(),
                    });
                    self.topic = Some(topic);
                    MessagesMode::Live
                };
            }
            MessagesPageMsg::PageSizeChanged(_idx) => {
                let page_size = match self.page_size_combo.model().get_active_elem() {
                    Some(ps) => *ps,
                    None => AVAILABLE_PAGE_SIZES[0],
                };
                self.page_size = page_size;
                self.page_size_combo.widget().queue_allocate();
            }
            MessagesPageMsg::Selection(size) => {
                let mut copy_content = String::from("PARTITION;OFFSET;VALUE;TIMESTAMP");
                let min_length = copy_content.len();
                for i in 0..size {
                    if self.messages_wrapper.selection_model.is_selected(i) {
                        let item = self.messages_wrapper.get_visible(i).unwrap();
                        let partition = item.borrow().partition;
                        let offset = item.borrow().offset;
                        let value = item.borrow().value.clone();
                        let clean_value =
                            match serde_json::from_str::<serde_json::Value>(value.as_str()) {
                                Ok(json) => json.to_string(),
                                Err(_) => value.replace('\n', ""),
                            };
                        let timestamp = item.borrow().timestamp;
                        let copy_text = format!(
                            "\n{};{};{};{}",
                            partition,
                            offset,
                            clean_value,
                            timestamp.unwrap_or_default()
                        );
                        copy_content.push_str(copy_text.as_str());
                        info!("selected offset[{}]", copy_text);
                    }
                }
                if copy_content.len() > min_length {
                    DisplayManager::get()
                        .default_display()
                        .unwrap()
                        .clipboard()
                        .set_text(copy_content.as_str());
                }
            }
            MessagesPageMsg::Open(connection, topic) => {
                let conn_id = &connection.id.unwrap();
                let topic_name = &topic.name.clone();
                self.connection = Some(connection);
                let mut repo = Repository::new();
                let maybe_topic = repo.find_topic(*conn_id, topic_name);
                self.topic = maybe_topic.clone().or(Some(topic));
                let toggled = maybe_topic.is_some();
                let cache_ts = maybe_topic
                    .and_then(|t| {
                        t.cached.map(|ts| {
                            Utc.timestamp_millis_opt(ts)
                                .unwrap()
                                .with_timezone(&America::Sao_Paulo)
                                .format(DATE_TIME_FORMAT)
                                .to_string()
                        })
                    })
                    .unwrap_or_default();
                widgets.cache_timestamp.set_label(&cache_ts);
                widgets.cache_timestamp.set_visible(true);
                widgets.btn_cache_toggle.set_active(toggled);
                widgets.pag_total_entry.set_text("");
                widgets.pag_current_entry.set_text("");
                widgets.pag_last_entry.set_text("");
                self.messages_wrapper.clear();
                self.page_size_combo.widget().queue_allocate();
                sender.input(MessagesPageMsg::ToggleMode(toggled));
            }
            MessagesPageMsg::GetMessages => {
                STATUS_BROKER.send(StatusBarMsg::Start);
                on_loading(widgets, false);
                let mode = self.mode;
                self.mode = match self.mode {
                    MessagesMode::Cached { refresh: _ } => MessagesMode::Cached { refresh: false },
                    MessagesMode::Live => {
                        self.messages_wrapper.clear();
                        MessagesMode::Live
                    }
                };
                let topic = self.topic.clone().unwrap();
                let conn = self.connection.clone().unwrap();
                if self.token.is_cancelled() {
                    self.token = CancellationToken::new();
                }
                let page_size = self.page_size;
                let token = self.token.clone();
                widgets.pag_current_entry.set_text("0");
                sender.oneshot_command(async move {
                    // Run async background task
                    let messages_worker = MessagesWorker::new();
                    let result = &messages_worker
                        .get_messages(
                            token,
                            &MessagesRequest {
                                mode,
                                connection: conn,
                                topic: topic.clone(),
                                page_operation: PageOp::Next,
                                page_size,
                                offset_partition: (0, 0),
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesPageMsg::GetNextMessages => {
                STATUS_BROKER.send(StatusBarMsg::Start);
                on_loading(widgets, false);
                let mode = self.mode;
                let topic = self.topic.clone().unwrap();
                let conn = self.connection.clone().unwrap();
                if self.token.is_cancelled() {
                    self.token = CancellationToken::new();
                }
                let page_size = self.page_size;
                let (offset, partition) = (
                    widgets
                        .last_offset
                        .text()
                        .to_string()
                        .parse::<usize>()
                        .unwrap(),
                    widgets
                        .last_partition
                        .text()
                        .to_string()
                        .parse::<usize>()
                        .unwrap(),
                );
                let token = self.token.clone();
                info!(
                    "getting next messages [page_size={}, last_offset={}, last_partition={}]",
                    page_size, offset, partition
                );
                sender.oneshot_command(async move {
                    // Run async background task
                    let messages_worker = MessagesWorker::new();
                    let result = &messages_worker
                        .get_messages(
                            token,
                            &MessagesRequest {
                                mode,
                                connection: conn,
                                topic: topic.clone(),
                                page_operation: PageOp::Next,
                                page_size,
                                offset_partition: (offset, partition),
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesPageMsg::GetPreviousMessages => {
                STATUS_BROKER.send(StatusBarMsg::Start);
                on_loading(widgets, false);
                let mode = self.mode;
                let topic = self.topic.clone().unwrap();
                let conn = self.connection.clone().unwrap();
                if self.token.is_cancelled() {
                    self.token = CancellationToken::new();
                }
                let page_size = self.page_size;
                let (offset, partition) = (
                    widgets
                        .first_offset
                        .text()
                        .to_string()
                        .parse::<usize>()
                        .unwrap(),
                    widgets
                        .first_partition
                        .text()
                        .to_string()
                        .parse::<usize>()
                        .unwrap(),
                );
                let token = self.token.clone();
                sender.oneshot_command(async move {
                    // Run async background task
                    let messages_worker = MessagesWorker::new();
                    let result = &messages_worker
                        .get_messages(
                            token,
                            &MessagesRequest {
                                mode,
                                connection: conn,
                                topic: topic.clone(),
                                page_operation: PageOp::Prev,
                                page_size,
                                offset_partition: (offset, partition),
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesPageMsg::RefreshMessages => {
                info!("refreshing cached messages");
                self.mode = MessagesMode::Cached { refresh: true };
                sender.input(MessagesPageMsg::GetMessages);
            }
            MessagesPageMsg::StopGetMessages => {
                info!("cancelling get messages...");
                self.token.cancel();
                on_loading(widgets, true);
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some("Operation cancelled!".to_string()),
                });
            }
            MessagesPageMsg::UpdateMessage(message) => {
                self.messages_wrapper.append(MessageListItem::new(message));
            }
            MessagesPageMsg::UpdateMessages(response) => {
                let total = response.total;
                self.topic = response.topic.clone();
                match self.mode {
                    MessagesMode::Live => info!("no need to cleanup list on live mode"),
                    MessagesMode::Cached { refresh: _ } => self.messages_wrapper.clear(),
                }
                self.headers_wrapper.clear();
                widgets.value_source_view.buffer().set_text("");
                fill_pagination(
                    response.page_operation,
                    widgets,
                    total,
                    response.page_size,
                    response.messages.first(),
                    response.messages.last(),
                );
                self.messages_wrapper.extend_from_iter(
                    response
                        .messages
                        .iter()
                        .map(|m| MessageListItem::new(m.clone())),
                );
                widgets.value_source_view.buffer().set_text("");
                let cache_ts = response
                    .topic
                    .and_then(|t| {
                        t.cached.map(|ts| {
                            Utc.timestamp_millis_opt(ts)
                                .unwrap()
                                .with_timezone(&America::Sao_Paulo)
                                .format(DATE_TIME_FORMAT)
                                .to_string()
                        })
                    })
                    .unwrap_or(String::default());
                widgets.cache_timestamp.set_label(&cache_ts);
                widgets.cache_timestamp.set_visible(true);
                on_loading(widgets, true);
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some(format!("{} messages loaded!", self.messages_wrapper.len())),
                });
            }
            MessagesPageMsg::OpenMessage(message_idx) => {
                let item = self.messages_wrapper.get_visible(message_idx).unwrap();
                let message_text = item.borrow().value.clone();

                let buffer = widgets
                    .value_source_view
                    .buffer()
                    .downcast::<sourceview::Buffer>()
                    .expect("sourceview was not backed by sourceview buffer");

                let valid_json: Result<serde_json::Value, _> =
                    serde_json::from_str(message_text.as_str());
                let (language, formatted_text) = match valid_json {
                    Ok(jt) => (
                        sourceview::LanguageManager::default().language("json"),
                        serde_json::to_string_pretty(&jt).unwrap(),
                    ),
                    Err(_) => (
                        sourceview::LanguageManager::default().language("text"),
                        message_text,
                    ),
                };
                buffer.set_language(language.as_ref());
                buffer.set_text(formatted_text.as_str());

                self.headers_wrapper.clear();
                for header in item.borrow().headers.iter() {
                    self.headers_wrapper
                        .append(HeaderListItem::new(header.clone()));
                }
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
            CommandMsg::Data(messages) => sender.input(MessagesPageMsg::UpdateMessages(messages)),
        }
    }
}

fn on_loading(widgets: &mut MessagesPageModelWidgets, enabled: bool,) {
    widgets.btn_next_page.set_sensitive(enabled);
    widgets.btn_previous_page.set_sensitive(enabled);
    widgets.btn_get_messages.set_sensitive(enabled);
    widgets.btn_cache_refresh.set_sensitive(enabled);
    widgets.btn_cache_toggle.set_sensitive(enabled);
}

fn fill_pagination(
    page_op: PageOp,
    widgets: &mut MessagesPageModelWidgets,
    total: usize,
    page_size: u16,
    first: Option<&KrustMessage>,
    last: Option<&KrustMessage>,
) {
    let current_page: usize = widgets
        .pag_current_entry
        .text()
        .to_string()
        .parse::<usize>()
        .unwrap_or_default();
    let current_page = match page_op {
        PageOp::Next => current_page + 1,
        PageOp::Prev => current_page - 1,
    };
    widgets.pag_total_entry.set_text(total.to_string().as_str());
    widgets
        .pag_current_entry
        .set_text(current_page.to_string().as_str());
    let pages = ((total as f64) / (page_size as f64)).ceil() as usize;
    widgets.pag_last_entry.set_text(pages.to_string().as_str());
    match (first, last) {
        (Some(first), Some(last)) => {
            let first_offset = first.offset;
            let first_partition = first.partition;
            let last_offset = last.offset;
            let last_partition = last.partition;
            widgets
                .first_offset
                .set_text(first_offset.to_string().as_str());
            widgets
                .first_partition
                .set_text(first_partition.to_string().as_str());
            widgets
                .last_offset
                .set_text(last_offset.to_string().as_str());
            widgets
                .last_partition
                .set_text(last_partition.to_string().as_str());
        }
        (_, _) => (),
    }
    debug!("fill pagination of current page {}", current_page);
    match current_page {
        1 => {
            widgets.btn_previous_page.set_sensitive(false);
            widgets.btn_next_page.set_sensitive(true);
        }
        n if n >= pages => {
            widgets.btn_next_page.set_sensitive(false);
            widgets.btn_previous_page.set_sensitive(true);
        }
        _ => {
            widgets.btn_next_page.set_sensitive(true);
            widgets.btn_previous_page.set_sensitive(true);
        }
    }
}
