#![allow(deprecated)]
use std::borrow::Borrow;

// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use chrono::{TimeZone, Utc};
use chrono_tz::America;
use csv::StringRecord;
use gtk::{
    gdk::{DisplayManager, Rectangle},
    ColumnViewSorter,
};
use relm4::{
    actions::{RelmAction, RelmActionGroup},
    factory::{DynamicIndex, FactoryComponent},
    typed_view::column::TypedColumnView,
    *,
};
use relm4_components::simple_combo_box::SimpleComboBox;
use sourceview::prelude::*;
use sourceview5 as sourceview;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace};

use crate::{
    backend::{
        kafka::KafkaFetch,
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

use super::lists::MessageKeyColumn;

// page actions
relm4::new_action_group!(pub MessagesPageActionGroup, "messages_page");
relm4::new_stateless_action!(pub MessagesSearchAction, MessagesPageActionGroup, "search");

relm4::new_action_group!(pub(super) MessagesListActionGroup, "messages-list");
relm4::new_stateless_action!(pub(super) CopyMessagesAsCsv, MessagesListActionGroup, "copy-messages-as-csv");
relm4::new_stateless_action!(pub(super) CopyMessagesValue, MessagesListActionGroup, "copy-messages-value");
relm4::new_stateless_action!(pub(super) CopyMessagesKey, MessagesListActionGroup, "copy-messages-key");

#[derive(Debug)]
pub struct MessagesTabModel {
    token: CancellationToken,
    pub topic: Option<KrustTopic>,
    mode: MessagesMode,
    pub connection: Option<KrustConnection>,
    messages_wrapper: TypedColumnView<MessageListItem, gtk::MultiSelection>,
    headers_wrapper: TypedColumnView<HeaderListItem, gtk::NoSelection>,
    page_size_combo: Controller<SimpleComboBox<u16>>,
    page_size: u16,
    fetch_type_combo: Controller<SimpleComboBox<KafkaFetch>>,
    fetch_type: KafkaFetch,
    max_messages: f64,
    messages_menu_popover: gtk::PopoverMenu,
}

pub struct MessagesTabInit {
    pub topic: KrustTopic,
    pub connection: KrustConnection,
}
#[derive(Debug)]
pub enum Copy {
    AllAsCsv,
    Value,
    Key,
}

#[derive(Debug)]
pub enum MessagesTabMsg {
    Open(KrustConnection, KrustTopic),
    GetMessages,
    GetNextMessages,
    GetPreviousMessages,
    StopGetMessages,
    RefreshMessages,
    UpdateMessages(MessagesResponse),
    OpenMessage(u32),
    SearchMessages,
    LiveSearchMessages(String),
    PageSizeChanged(usize),
    FetchTypeChanged(usize),
    ToggleMode(bool),
    DigitsOnly(f64),
    CopyMessages(Copy),
}

#[derive(Debug)]
pub enum CommandMsg {
    Data(MessagesResponse),
    CopyToClipboard(String),
}

const AVAILABLE_PAGE_SIZES: [u16; 6] = [50, 100, 500, 1000, 2000, 5000];

#[relm4::factory(pub)]
impl FactoryComponent for MessagesTabModel {
    type Init = MessagesTabInit;
    type Input = MessagesTabMsg;
    type Output = ();
    type CommandOutput = CommandMsg;
    type ParentWidget = adw::TabView;

    menu! {
        messages_menu: {
            section! {
                "_Copy as CSV" => CopyMessagesAsCsv,
                "_Copy value" => CopyMessagesValue,
                "_Copy key" => CopyMessagesKey,
            }
        }
    }

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
                container_add = &self.messages_menu_popover.clone() {
                    set_menu_model: Some(&messages_menu),
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
                        #[name(btn_get_messages)]
                        gtk::Button {
                            set_icon_name: "media-playback-start-symbolic",
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::GetMessages);
                            },
                        },
                        #[name(btn_stop_messages)]
                        gtk::Button {
                            set_icon_name: "media-playback-stop-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::StopGetMessages);
                            },
                        },
                        #[name(btn_cache_refresh)]
                        gtk::Button {
                            set_icon_name: "media-playlist-repeat-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::RefreshMessages);
                            },
                        },
                        #[name(btn_cache_toggle)]
                        gtk::ToggleButton {
                            set_margin_start: 5,
                            set_label: "Cache",
                            add_css_class: "krust-toggle",
                            connect_toggled[sender] => move |btn| {
                                sender.input(MessagesTabMsg::ToggleMode(btn.is_active()));
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
                        set_halign: gtk::Align::End,
                        set_hexpand: true,
                        #[name(messages_search_entry)]
                        gtk::SearchEntry {
                            set_hexpand: false,
                            set_halign: gtk::Align::Fill,
                            connect_search_changed[sender] => move |entry| {
                                sender.clone().input(MessagesTabMsg::LiveSearchMessages(entry.text().to_string()));
                            },
                            connect_activate[sender] => move |_entry| {
                                sender.input(MessagesTabMsg::SearchMessages);
                            },
                        },
                        self.fetch_type_combo.widget() -> &gtk::ComboBoxText {
                            set_margin_start: 5,
                        },
                    },
                },
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_propagate_natural_width: true,
                    self.messages_wrapper.view.clone() -> gtk::ColumnView {
                        set_vexpand: true,
                        set_hexpand: true,
                        set_show_row_separators: true,
                        set_show_column_separators: true,
                        set_single_click_activate: false,
                        set_enable_rubberband: true,
                    },
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
                            self.headers_wrapper.view.clone() -> gtk::ColumnView {
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
                            self.page_size_combo.widget() -> &gtk::ComboBoxText {
                                set_margin_start: 5,
                            },
                            #[name(btn_previous_page)]
                            gtk::Button {
                                set_margin_start: 5,
                                set_icon_name: "go-previous",
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesTabMsg::GetPreviousMessages);
                                },
                            },
                            #[name(btn_next_page)]
                            gtk::Button {
                                set_margin_start: 5,
                                set_icon_name: "go-next",
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesTabMsg::GetNextMessages);
                                },
                            },
                        },
                        #[name(live_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            gtk::Label {
                                set_label: "Max messages (per partition)",
                                set_margin_start: 5,
                            },
                            #[name(max_messages)]
                            gtk::SpinButton {
                                set_margin_start: 5,
                                set_width_chars: 10,
                                set_numeric: true,
                                set_increments: (1000.0, 10000.0),
                                set_range: (1.0, 100000.0),
                                set_value: self.max_messages,
                                set_digits: 0,
                                connect_value_changed[sender] => move |sbtn| {
                                    sender.input(MessagesTabMsg::DigitsOnly(sbtn.value()));
                                },
                            },
                        },
                    },
                },
            },
        },

    }

    fn init_model(open: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        // Initialize the messages ListView wrapper
        let mut messages_wrapper = TypedColumnView::<MessageListItem, gtk::MultiSelection>::new();
        messages_wrapper.append_column::<MessagePartitionColumn>();
        messages_wrapper.append_column::<MessageOfssetColumn>();
        messages_wrapper.append_column::<MessageKeyColumn>();
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
            .forward(sender.input_sender(), MessagesTabMsg::PageSizeChanged);
        page_size_combo.widget().queue_allocate();
        let fetch_type_combo = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: KafkaFetch::VALUES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(sender.input_sender(), MessagesTabMsg::FetchTypeChanged);

        let messages_popover_menu = gtk::PopoverMenu::builder().build();
        let mut messages_actions = RelmActionGroup::<MessagesListActionGroup>::new();
        let messages_menu_sender = sender.input_sender().clone();
        let menu_copy_all_csv_action = RelmAction::<CopyMessagesAsCsv>::new_stateless(move |_| {
            messages_menu_sender
                .send(MessagesTabMsg::CopyMessages(Copy::AllAsCsv))
                .unwrap();
        });
        let messages_menu_sender = sender.input_sender().clone();
        let menu_copy_value_action = RelmAction::<CopyMessagesValue>::new_stateless(move |_| {
            messages_menu_sender
                .send(MessagesTabMsg::CopyMessages(Copy::Value))
                .unwrap();
        });
        let messages_menu_sender = sender.input_sender().clone();
        let menu_copy_key_action = RelmAction::<CopyMessagesKey>::new_stateless(move |_| {
            messages_menu_sender
                .send(MessagesTabMsg::CopyMessages(Copy::Key))
                .unwrap();
        });
        messages_actions.add_action(menu_copy_all_csv_action);
        messages_actions.add_action(menu_copy_value_action);
        messages_actions.add_action(menu_copy_key_action);
        messages_actions.register_for_widget(&messages_popover_menu);
        let model = MessagesTabModel {
            token: CancellationToken::new(),
            mode: MessagesMode::Live,
            topic: Some(open.topic),
            connection: Some(open.connection),
            messages_wrapper,
            headers_wrapper,
            page_size_combo,
            page_size: AVAILABLE_PAGE_SIZES[0],
            fetch_type_combo,
            fetch_type: KafkaFetch::default(),
            max_messages: 1000.0,
            messages_menu_popover: messages_popover_menu,
        };
        let messages_view = &model.messages_wrapper.view;
        let _headers_view = &model.headers_wrapper.view;
        let _sender_for_selection = sender.clone();
        messages_view
            .model()
            .unwrap()
            .connect_selection_changed(move |_selection_model, _, _| {
                //sender_for_selection.input(MessagesTabMsg::Selection(selection_model.n_items()));
            });
        let sender_for_activate = sender.clone();
        messages_view.connect_activate(move |_view, idx| {
            sender_for_activate.input(MessagesTabMsg::OpenMessage(idx));
        });

        messages_view
            .sorter()
            .unwrap()
            .connect_changed(move |sorter, change| {
                let order = sorter.order();
                let csorter: &ColumnViewSorter = sorter.downcast_ref().unwrap();
                info!("sort order changed: {:?}:{:?}", change, order);
                for i in 0..=csorter.n_sort_columns() {
                    let (cvc, sort) = csorter.nth_sort_column(i);
                    info!(
                        "column[{:?}]sort[{:?}]",
                        cvc.map(|col| { col.title() }),
                        sort
                    );
                }
            });

        sender.input(MessagesTabMsg::Open(
            model.connection.clone().unwrap(),
            model.topic.clone().unwrap(),
        ));
        model
    }

    fn post_view(&self, widgets: &mut Self::Widgets) {
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
        widgets.max_messages.set_increments(1000.0, 10000.0);
        // Shortcuts
        // let mut actions = RelmActionGroup::<MessagesPageActionGroup>::new();

        // let messages_search_entry = widgets.messages_search_entry.clone();
        // let search_action = {
        //     let messages_search_btn = widgets.messages_search_btn.clone();
        //     RelmAction::<MessagesSearchAction>::new_stateless(move |_| {
        //         messages_search_btn.emit_clicked();
        //     })
        // };
        // actions.add_action(search_action);
        // actions.register_for_widget(messages_search_entry);

        //self.messages_menu_popover.set_menu_model(widgets.menu)
        // Create a click gesture
        let gesture = gtk::GestureClick::new();

        // Set the gestures button to the right mouse button (=3)
        gesture.set_button(gtk::gdk::ffi::GDK_BUTTON_SECONDARY as u32);

        // Assign your handler to an event of the gesture (e.g. the `pressed` event)
        let messages_menu = self.messages_menu_popover.clone();
        gesture.connect_pressed(move |gesture, _n, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let x = x as i32;
            let y = y as i32;
            info!("ColumnView: Right mouse button pressed [x={},y={}]", x, y);
            messages_menu.set_pointing_to(Some(&Rectangle::new(x, y + 55, 1, 1)));
            messages_menu.popup();
        });
        self.messages_wrapper.view.add_controller(gesture);
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: MessagesTabMsg,
        sender: FactorySender<Self>,
    ) {
        match msg {
            MessagesTabMsg::DigitsOnly(value) => {
                self.max_messages = value;
                info!("Max messages:{}", self.max_messages);
            }
            MessagesTabMsg::ToggleMode(toggle) => {
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
                        total: None,
                        favourite: cloned_topic.favourite.clone(),
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
            MessagesTabMsg::PageSizeChanged(_idx) => {
                let page_size = match self.page_size_combo.model().get_active_elem() {
                    Some(ps) => *ps,
                    None => AVAILABLE_PAGE_SIZES[0],
                };
                self.page_size = page_size;
                self.page_size_combo.widget().queue_allocate();
            }
            MessagesTabMsg::FetchTypeChanged(_idx) => {
                let fetch_type = match self.fetch_type_combo.model().get_active_elem() {
                    Some(ps) => ps.clone(),
                    None => KafkaFetch::default(),
                };
                self.fetch_type = fetch_type;
                self.fetch_type_combo.widget().queue_allocate();
            }
            MessagesTabMsg::CopyMessages(copy) => {
                info!("copy selected messages");
                let topic = self.topic.clone().unwrap().name;
                let mut selected_items = vec![];
                for i in 0..self.messages_wrapper.selection_model.n_items() {
                    if self.messages_wrapper.selection_model.is_selected(i) {
                        let item = self.messages_wrapper.get_visible(i).unwrap();
                        selected_items.push(KrustMessage {
                            headers: item.borrow().headers.clone(),
                            topic: topic.clone(),
                            partition: item.borrow().partition,
                            offset: item.borrow().offset,
                            key: Some(item.borrow().key.clone()),
                            value: item.borrow().value.clone(),
                            timestamp: item.borrow().timestamp.clone(),
                        });
                    }
                }
                sender.spawn_oneshot_command(move || {
                    let data = match copy {
                        Copy::AllAsCsv => copy_all_as_csv(&selected_items),
                        Copy::Value => copy_value(&selected_items),
                        Copy::Key => copy_key(&selected_items),
                    };
                    if let Ok(data) = data {
                        CommandMsg::CopyToClipboard(data)
                    } else {
                        CommandMsg::CopyToClipboard(String::default())
                    }
                });
                // if let Ok(data) = data {
                //     DisplayManager::get()
                //         .default_display()
                //         .unwrap()
                //         .clipboard()
                //         .set_text(data.as_str());
                // }
            }
            MessagesTabMsg::Open(connection, topic) => {
                let conn_id = &connection.id.unwrap();
                let topic_name = &topic.name.clone();
                self.connection = Some(connection);
                let mut repo = Repository::new();
                let maybe_topic = repo.find_topic(*conn_id, topic_name);
                self.topic = maybe_topic.clone().or(Some(topic));
                let toggled = match &maybe_topic {
                    Some(t) => t.cached.is_some(),
                    None => false,
                };
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
                sender.input(MessagesTabMsg::ToggleMode(toggled));
            }
            MessagesTabMsg::LiveSearchMessages(term) => {
                match self.mode.clone() {
                    MessagesMode::Live => {
                        self.messages_wrapper.clear_filters();
                        let search_term = term.clone();
                        self.messages_wrapper
                            .add_filter(move |item| item.value.contains(search_term.as_str()));
                    }
                    MessagesMode::Cached { refresh: _ } => (),
                };
            }
            MessagesTabMsg::SearchMessages => {
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::GetMessages => {
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
                let search = get_search_term(widgets);
                let fetch = self.fetch_type.clone();
                let max_messages: i64 = self.max_messages as i64;
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
                                search: search,
                                fetch,
                                max_messages,
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesTabMsg::GetNextMessages => {
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
                let search = get_search_term(widgets);
                let token = self.token.clone();
                let fetch = self.fetch_type.clone();
                let max_messages: i64 = self.max_messages as i64;
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
                                search: search,
                                fetch,
                                max_messages,
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesTabMsg::GetPreviousMessages => {
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
                let search = get_search_term(widgets);
                let fetch = self.fetch_type.clone();
                let max_messages: i64 = self.max_messages as i64;
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
                                search: search,
                                fetch,
                                max_messages,
                            },
                        )
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesTabMsg::RefreshMessages => {
                info!("refreshing cached messages");
                self.mode = MessagesMode::Cached { refresh: true };
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::StopGetMessages => {
                info!("cancelling get messages...");
                self.token.cancel();
                on_loading(widgets, true);
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some("Operation cancelled!".to_string()),
                });
            }
            MessagesTabMsg::UpdateMessages(response) => {
                let total = response.total;
                self.topic = response.topic.clone();
                match self.mode {
                    MessagesMode::Live => info!("no need to cleanup list on live mode"),
                    MessagesMode::Cached { refresh: _ } => self.messages_wrapper.clear(),
                }
                self.headers_wrapper.clear();
                widgets.value_source_view.buffer().set_text("");
                on_loading(widgets, true);
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
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some(format!("{} messages loaded!", self.messages_wrapper.len())),
                });
            }
            MessagesTabMsg::OpenMessage(message_idx) => {
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

    fn update_cmd(&mut self, message: Self::CommandOutput, sender: FactorySender<Self>) {
        match message {
            CommandMsg::Data(messages) => sender.input(MessagesTabMsg::UpdateMessages(messages)),
            CommandMsg::CopyToClipboard(data) => {
                info!("setting text to clipboard");
                DisplayManager::get()
                    .default_display()
                    .unwrap()
                    .clipboard()
                    .set_text(data.as_str());
            }
        }
    }
}

fn get_search_term(widgets: &mut MessagesTabModelWidgets) -> Option<String> {
    let search: Option<String> = widgets.messages_search_entry.text().try_into().ok();
    let search = search.clone().unwrap_or_default();
    let search_txt = search.trim();
    if search_txt.is_empty() {
        None
    } else {
        Some(search_txt.to_string())
    }
}

fn on_loading(widgets: &mut MessagesTabModelWidgets, enabled: bool) {
    widgets.btn_get_messages.set_sensitive(enabled);
    widgets.btn_cache_refresh.set_sensitive(enabled);
    widgets.btn_cache_toggle.set_sensitive(enabled);
    widgets.max_messages.set_sensitive(enabled);
}

fn fill_pagination(
    page_op: PageOp,
    widgets: &mut MessagesTabModelWidgets,
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

fn copy_all_as_csv(
    selected_items: &Vec<KrustMessage>,
) -> Result<String, std::string::FromUtf8Error> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b';')
        .quote_style(csv::QuoteStyle::NonNumeric)
        .from_writer(vec![]);
    let _ = wtr.write_record(&["PARTITION", "OFFSET", "KEY", "VALUE", "TIMESTAMP"]);
    for item in selected_items {
        let partition = item.partition;
        let offset = item.offset;
        let key = item.key.clone();
        let value = item.value.clone();
        let clean_value = match serde_json::from_str::<serde_json::Value>(value.as_str()) {
            Ok(json) => json.to_string(),
            Err(_) => value.replace('\n', ""),
        };
        let timestamp = item.borrow().timestamp;
        let record = StringRecord::from(vec![
            partition.to_string(),
            offset.to_string(),
            key.unwrap_or_default(),
            clean_value,
            timestamp.unwrap_or_default().to_string(),
        ]);
        let _ = wtr.write_record(&record);
    }
    let data = String::from_utf8(wtr.into_inner().unwrap_or_default());
    data
}
fn copy_value(selected_items: &Vec<KrustMessage>) -> Result<String, std::string::FromUtf8Error> {
    let mut copy_content = String::default();
    for item in selected_items {
        let value = item.value.clone();
        let clean_value = match serde_json::from_str::<serde_json::Value>(value.as_str()) {
            Ok(json) => json.to_string(),
            Err(_) => value.replace('\n', ""),
        };
        let copy_text = format!("{}\n", clean_value);
        copy_content.push_str(&copy_text.as_str());
    }
    Ok(copy_content)
}
fn copy_key(selected_items: &Vec<KrustMessage>) -> Result<String, std::string::FromUtf8Error> {
    let mut copy_content = String::default();
    for item in selected_items {
        let key = item.key.clone();
        let copy_text = format!("{}\n", key.unwrap_or_default());
        copy_content.push_str(&copy_text.as_str());
    }
    Ok(copy_content)
}
