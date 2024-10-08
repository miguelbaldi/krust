#![allow(deprecated)]
// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use std::borrow::Borrow;
use std::str::FromStr;

// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use chrono::{TimeZone, Utc};
use chrono_tz::America;
use csv::StringRecord;
use gtk::{gdk::Rectangle, ColumnViewSorter};
use gtk::{prelude::*, ColumnViewColumn, SortType};
use relm4::{
    actions::{RelmAction, RelmActionGroup},
    factory::{DynamicIndex, FactoryComponent},
    typed_view::column::TypedColumnView,
    *,
};

use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};
use tokio_util::sync::CancellationToken;
use tracing::*;
use uuid::Uuid;

use crate::backend::kafka::KafkaBackend;
use crate::backend::repository::{KrustTopicCache, MessagesSearchOrder};
use crate::backend::settings::Settings;
use crate::backend::worker::MessagesTotalCounterRequest;
use crate::component::settings_dialog::MessagesSortOrder;
use crate::component::task_manager::{Task, TaskManagerMsg, TaskVariant, TASK_MANAGER_BROKER};
use crate::modals::utils::show_error_alert;
use crate::{
    backend::{
        kafka::KafkaFetch,
        repository::{KrustConnection, KrustMessage, KrustTopic},
        worker::{
            MessagesCleanupRequest, MessagesMode, MessagesRequest, MessagesResponse, MessagesWorker,
        },
    },
    component::{
        messages::lists::{
            MessageListItem, MessageOffsetColumn, MessagePartitionColumn, MessageTimestampColumn,
            MessageValueColumn,
        },
        status_bar::{StatusBarMsg, STATUS_BROKER},
    },
    Repository,
};
use crate::{AppMsg, TOASTER_BROKER};

use super::message_viewer::{MessageViewerModel, MessageViewerMsg};
use super::messages_cache_settings_dialog::{
    MessagesCacheSettingsDialogModel, MessagesCacheSettingsDialogMsg,
    MessagesCacheSettingsDialogOutput,
};
use super::messages_send_dialog::MessagesSendDialogMsg;
use super::{lists::MessageKeyColumn, messages_send_dialog::MessagesSendDialogModel};
use copypasta::{ClipboardContext, ClipboardProvider};
use humansize::{format_size, DECIMAL};

// page actions
relm4::new_action_group!(pub MessagesPageActionGroup, "messages_page");
relm4::new_stateless_action!(pub MessagesSearchAction, MessagesPageActionGroup, "search");

relm4::new_action_group!(pub(super) MessagesListActionGroup, "messages-list");
relm4::new_stateless_action!(pub(super) CopyMessagesAsCsv, MessagesListActionGroup, "copy-messages-as-csv");
relm4::new_stateless_action!(pub(super) CopyMessagesKeyValue, MessagesListActionGroup, "copy-messages-key-value");
relm4::new_stateless_action!(pub(super) CopyMessagesValue, MessagesListActionGroup, "copy-messages-value");
relm4::new_stateless_action!(pub(super) CopyMessagesKey, MessagesListActionGroup, "copy-messages-key");
relm4::new_stateless_action!(pub(super) ResendMessagesKeyValue, MessagesListActionGroup, "resend-messages-key-value");
relm4::new_stateless_action!(pub(super) ResendMessagesValue, MessagesListActionGroup, "resend-messages-value");

pub struct MessagesTabModel {
    token: CancellationToken,
    pub topic: Option<KrustTopic>,
    mode: MessagesMode,
    pub connection: Option<KrustConnection>,
    messages_wrapper: TypedColumnView<MessageListItem, gtk::MultiSelection>,
    message_viewer: Controller<MessageViewerModel>,
    page_size_combo: Controller<SimpleComboBox<u16>>,
    page_size: u16,
    fetch_type_combo: Controller<SimpleComboBox<KafkaFetch>>,
    fetch_type: KafkaFetch,
    max_messages: f64,
    messages_menu_popover: gtk::PopoverMenu,
    add_messages: Controller<MessagesSendDialogModel>,
    clipboard: Box<dyn ClipboardProvider>,
    cache_search_order: Option<MessagesSearchOrder>,
    cache_settings_dialog: Controller<MessagesCacheSettingsDialogModel>,
    cache_settings: Option<KrustTopicCache>,
}

pub struct MessagesTabInit {
    pub topic: KrustTopic,
    pub connection: KrustConnection,
}
#[derive(Debug)]
pub enum Copy {
    AllAsCsv,
    KeyValue,
    Value,
    Key,
}

#[derive(Debug)]
pub enum MessagesTabMsg {
    Open(Box<KrustConnection>, Box<KrustTopic>),
    GetMessages,
    GetNextMessages,
    GetPreviousMessages,
    GotoPage,
    StopGetMessages,
    RefreshCache,
    DestroyCache,
    RefreshTotalCounter,
    UpdateMessages(Box<MessagesResponse>),
    OpenMessage(u32),
    SearchMessages,
    LiveSearchMessages(String),
    PageSizeChanged(usize),
    FetchTypeChanged(usize),
    ToggleMode(bool),
    DigitsOnly(f64),
    CopyMessages(Copy),
    ResendMessages(Copy),
    AddMessages,
    SetCacheOrder(Option<String>, String),
    RefreshTopic,
    ShowCacheSettings,
    UpdateCacheSettings(KrustTopicCache),
}

#[derive(Debug)]
pub enum CommandMsg {
    Data(MessagesResponse),
    CopyToClipboard(String, String),
    RefreshTotalCounterResult(String, usize),
    MessagesResendResult(String, Option<()>),
}

pub const AVAILABLE_PAGE_SIZES: [u16; 7] = [1000, 2000, 5000, 7000, 10000, 20000, 50000];

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
                "_Copy key,value" => CopyMessagesKeyValue,
                "_Copy value" => CopyMessagesValue,
                "_Copy key" => CopyMessagesKey,
                "_Resend message(s) with key/value" => ResendMessagesKeyValue,
                "_Resend message(s) with value only" => ResendMessagesValue,
            }
        }
    }

    view! {
        #[root]
        #[name(main_panel)]
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
                            set_tooltip_text: Some("Show messages"),
                            set_icon_name: "media-playback-start-symbolic",
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::GetMessages);
                            },
                        },
                        #[name(btn_stop_messages)]
                        gtk::Button {
                            set_tooltip_text: Some("Stop current task"),
                            set_icon_name: "media-playback-stop-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::StopGetMessages);
                            },
                        },
                        #[name(btn_cache_refresh)]
                        gtk::Button {
                            set_tooltip_text: Some("Refresh cache"),
                            set_icon_name: "media-playlist-repeat-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::RefreshCache);
                            },
                        },
                        #[name(btn_send_messages)]
                        gtk::Button {
                            set_tooltip_text: Some("Send messages"),
                            set_icon_name: "list-add-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::AddMessages);
                            },
                        },
                        #[name(btn_cache_settings)]
                        gtk::Button {
                            set_tooltip_text: Some("Cache settings"),
                            set_icon_name: "emblem-system-symbolic",
                            set_margin_start: 5,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::ShowCacheSettings);
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
                            set_tooltip_text: Some("Last cache refresh timestamp"),
                            set_margin_start: 5,
                            set_label: "",
                            set_visible: false,
                            add_css_class: "cache-timestamp",
                        },
                        #[name(btn_cache_destroy)]
                        gtk::Button {
                            set_tooltip_text: Some("Destroy cache"),
                            set_icon_name: "edit-delete-symbolic",
                            set_margin_start: 5,
                            add_css_class: "destructive-action",
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesTabMsg::DestroyCache);
                            },
                        },
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
                            set_width_chars: 50,
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
                    #[name = "messages_view" ]
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
                    set_stack: Some(&message_viewer_stack),
                },
                append: message_viewer_stack = &self.message_viewer.widget().clone() -> gtk::Stack {},
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
                        #[name(pag_total_label)]
                        gtk::Label {
                            set_label: "Messages"
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
                        #[name(live_centered_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            #[name(total_counter_label)]
                            gtk::Label {
                                set_label: "Total",
                                set_margin_start: 5,
                            },
                            #[name(total_counter_entry)]
                            gtk::Entry {
                                set_editable: false,
                                set_sensitive: true,
                                set_margin_start: 5,
                                set_width_chars: 10,
                            },
                            #[name(btn_total_counter_refresh)]
                            gtk::Button {
                                set_tooltip_text: Some("Refresh messages total counter"),
                                set_icon_name: "media-playlist-repeat-symbolic",
                                set_margin_start: 5,
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesTabMsg::RefreshTotalCounter);
                                },
                            },
                        },
                        #[name(cached_centered_controls)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            #[name(btn_goto_page)]
                            gtk::Button {
                                set_tooltip_text: Some("Refresh messages total counter"),
                                set_icon_name: "media-skip-forward",
                                set_margin_start: 5,
                                connect_clicked[sender] => move |_| {
                                    sender.input(MessagesTabMsg::GotoPage);
                                },
                            },
                            #[name(pag_current_entry)]
                            gtk::Entry {
                                set_editable: true,
                                set_sensitive: true,
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
        messages_wrapper.append_column::<MessageOffsetColumn>();
        messages_wrapper.append_column::<MessageKeyColumn>();
        messages_wrapper.append_column::<MessageValueColumn>();
        messages_wrapper.append_column::<MessageTimestampColumn>();

        // Initialize message viewer
        let message_viewer = MessageViewerModel::builder().launch(()).detach();
        let cache_settings = open.topic.cached.clone();
        let default_idx = cache_settings
            .clone()
            .map(|c| c.default_page_size as usize)
            .unwrap_or_default();
        let page_size_combo = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: AVAILABLE_PAGE_SIZES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(sender.input_sender(), MessagesTabMsg::PageSizeChanged);
        page_size_combo.widget().queue_allocate();
        let fetch_type_default_idx = 0;
        let fetch_type_combo = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: KafkaFetch::VALUES.to_vec(),
                active_index: Some(fetch_type_default_idx),
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
        let menu_copy_key_value_action =
            RelmAction::<CopyMessagesKeyValue>::new_stateless(move |_| {
                messages_menu_sender
                    .send(MessagesTabMsg::CopyMessages(Copy::KeyValue))
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
        let messages_menu_sender = sender.input_sender().clone();
        let menu_resend_key_value_action =
            RelmAction::<ResendMessagesKeyValue>::new_stateless(move |_| {
                messages_menu_sender
                    .send(MessagesTabMsg::ResendMessages(Copy::KeyValue))
                    .unwrap();
            });
        let messages_menu_sender = sender.input_sender().clone();
        let menu_resend_value_action =
            RelmAction::<ResendMessagesValue>::new_stateless(move |_| {
                messages_menu_sender
                    .send(MessagesTabMsg::ResendMessages(Copy::Value))
                    .unwrap();
            });
        messages_actions.add_action(menu_copy_all_csv_action);
        messages_actions.add_action(menu_copy_key_value_action);
        messages_actions.add_action(menu_copy_value_action);
        messages_actions.add_action(menu_copy_key_action);
        messages_actions.add_action(menu_resend_key_value_action);
        messages_actions.add_action(menu_resend_value_action);
        messages_actions.register_for_widget(&messages_popover_menu);

        let add_messages = MessagesSendDialogModel::builder()
            //.transient_for(main_application())
            .launch((Some(open.connection.clone()), Some(open.topic.clone())))
            .detach();
        let cache_settings_dialog = MessagesCacheSettingsDialogModel::builder()
            //.transient_for(main_application())
            .launch((open.connection.clone(), Some(open.topic.clone())))
            .forward(sender.input_sender(), |msg| match msg {
                MessagesCacheSettingsDialogOutput::Update(cache) => {
                    MessagesTabMsg::UpdateCacheSettings(cache)
                }
            });
        let clipboard = Box::new(ClipboardContext::new().unwrap());
        let model = MessagesTabModel {
            token: CancellationToken::new(),
            mode: MessagesMode::Live,
            topic: Some(open.topic),
            connection: Some(open.connection),
            messages_wrapper,
            message_viewer,
            page_size_combo,
            page_size: AVAILABLE_PAGE_SIZES[default_idx],
            fetch_type_combo,
            fetch_type: KafkaFetch::default(),
            max_messages: 1000.0,
            messages_menu_popover: messages_popover_menu,
            add_messages,
            clipboard,
            cache_search_order: None,
            cache_settings_dialog,
            cache_settings,
        };
        let messages_view = &model.messages_wrapper.view;
        let sender_for_selection = sender.clone();
        messages_view
            .model()
            .unwrap()
            .connect_selection_changed(move |selection_model, i, j| {
                let size = selection_model.selection().size();
                if size == 1 {
                    let selected = selection_model.selection().minimum();
                    trace!(
                        "messages_view::selection_changed[{}][{}][{}][{}]",
                        i,
                        j,
                        size,
                        selected
                    );
                    sender_for_selection.input(MessagesTabMsg::OpenMessage(selected));
                }
            });

        let snd = sender.clone();
        messages_view
            .sorter()
            .unwrap()
            .connect_changed(move |sorter, change| {
                let order = sorter.order();
                let csorter: &ColumnViewSorter = sorter.downcast_ref().unwrap();
                info!("sort order changed: {:?}:{:?}", change, order);
                if csorter.n_sort_columns() > 0 {
                    let (cvc, sort) = csorter.nth_sort_column(0);
                    if let Some(col) = cvc {
                        let col_name: Option<String> = col.title().map(|s| s.to_string());
                        if let Some(col_name) = col_name {
                            let col_name = match col_name.as_str() {
                                "Offset" => Some("offset"),
                                "Partition" => Some("partition"),
                                "Date/time (Timestamp)" => Some("timestamp"),
                                _ => None,
                            }
                            .map(|s| s.to_string());
                            let order = match sort {
                                SortType::Ascending => "ASC",
                                SortType::Descending => "DESC",
                                _ => "ASC",
                            }
                            .to_string();
                            info!("order selected column[{:?}]sort[{:?}]", col_name, order);
                            snd.input(MessagesTabMsg::SetCacheOrder(col_name, order));
                        }
                    }
                }
            });

        sender.input(MessagesTabMsg::Open(
            Box::new(model.connection.clone().unwrap()),
            Box::new(model.topic.clone().unwrap()),
        ));
        model
    }

    fn pre_view(&self, _widgets: &mut Self::Widgets) {
        trace!("messages_tab::pre_view");
    }

    fn post_view(&self, widgets: &mut Self::Widgets) {
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
            MessagesTabMsg::UpdateCacheSettings(cache) => {
                info!("topic cache settings updated! {:?}", cache);
                self.cache_settings = Some(cache.clone());
                let cache_settings = cache.clone();
                let default_idx = cache_settings.default_page_size as usize;
                self.page_size_combo
                    .emit(SimpleComboBoxMsg::SetActiveIdx(default_idx));
            }
            MessagesTabMsg::SetCacheOrder(maybe_column, order) => {
                let cache_messages_order =
                    maybe_column.map(|column| MessagesSearchOrder { column, order });
                self.cache_search_order = cache_messages_order;
                if let MessagesMode::Cached { refresh: _ } = self.mode {
                    sender.input(MessagesTabMsg::GetMessages);
                }
            }
            MessagesTabMsg::AddMessages => {
                self.add_messages.emit(MessagesSendDialogMsg::Show);
            }
            MessagesTabMsg::ShowCacheSettings => {
                self.cache_settings_dialog
                    .emit(MessagesCacheSettingsDialogMsg::Show);
            }
            MessagesTabMsg::DigitsOnly(value) => {
                self.max_messages = value;
                info!("Max messages:{}", self.max_messages);
            }
            MessagesTabMsg::ToggleMode(toggle) => {
                self.mode = if toggle {
                    widgets.cached_controls.set_visible(true);
                    widgets.cached_centered_controls.set_visible(true);
                    widgets.live_centered_controls.set_visible(false);
                    widgets.live_controls.set_visible(false);
                    widgets.btn_cache_refresh.set_visible(true);
                    widgets.btn_cache_destroy.set_visible(true);
                    widgets.btn_cache_settings.set_visible(true);
                    widgets.pag_total_label.set_text("Total");
                    MessagesMode::Cached { refresh: false }
                } else {
                    widgets.live_controls.set_visible(true);
                    widgets.btn_cache_refresh.set_visible(false);
                    widgets.btn_cache_destroy.set_visible(false);
                    widgets.btn_cache_settings.set_visible(false);
                    widgets.pag_total_label.set_text("Messages");
                    widgets.cached_controls.set_visible(false);
                    widgets.cached_centered_controls.set_visible(false);
                    widgets.live_centered_controls.set_visible(true);
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
                let cache = self.find_cache();
                if cache.is_some() {
                    sender.input(MessagesTabMsg::SearchMessages);
                }
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
                            timestamp: item.borrow().timestamp,
                        });
                    }
                }
                sender.spawn_oneshot_command(move || {
                    let id = Uuid::new_v4();
                    TOASTER_BROKER
                        .send(AppMsg::ShowToast(id.to_string(), "Copying...".to_string()));
                    let data = match copy {
                        Copy::AllAsCsv => copy_all_as_csv(&selected_items),
                        Copy::KeyValue => copy_key_value(&selected_items),
                        Copy::Value => copy_value(&selected_items),
                        Copy::Key => copy_key(&selected_items),
                    };
                    if let Ok(data) = data {
                        CommandMsg::CopyToClipboard(id.to_string(), data)
                    } else {
                        CommandMsg::CopyToClipboard(id.to_string(), String::default())
                    }
                });
            }
            MessagesTabMsg::ResendMessages(copy) => {
                info!("resend selected messages");
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
                            key: match copy {
                                Copy::KeyValue => Some(item.borrow().key.clone()),
                                _ => None,
                            },
                            value: item.borrow().value.clone(),
                            timestamp: item.borrow().timestamp,
                        });
                    }
                }
                selected_items.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
                let connection = self.connection.clone().unwrap();
                sender.oneshot_command(async move {
                    let id = Uuid::new_v4();
                    TOASTER_BROKER.send(AppMsg::ShowToast(
                        id.to_string(),
                        "Resending...".to_string(),
                    ));
                    debug!("sending messages::{:?}", &selected_items);
                    // Run async background task
                    let kafka = KafkaBackend::new(&connection);
                    kafka.send_messages(&topic, &selected_items).await;
                    CommandMsg::MessagesResendResult(id.to_string(), Some(()))
                });
            }
            MessagesTabMsg::Open(connection, topic) => {
                let timestamp_format = Settings::read().unwrap_or_default().timestamp_formatter();
                let conn_id = &connection.id.unwrap();
                let topic_name = &topic.name.clone();
                self.connection = Some(*connection);
                let mut repo = Repository::new();
                let maybe_topic = repo.find_topic(*conn_id, topic_name);
                self.topic = maybe_topic.clone().or(Some(*topic));
                self.cache_settings = self.topic.clone().and_then(|t| t.cached);
                let toggled = match &maybe_topic {
                    Some(t) => t.cached.is_some(),
                    None => false,
                };
                let cache_ts = maybe_topic
                    .and_then(|t| {
                        t.cached.map(|c| {
                            c.last_updated.map(|ts| {
                                Utc.timestamp_millis_opt(ts)
                                    .unwrap()
                                    .with_timezone(&America::Sao_Paulo)
                                    .format(&timestamp_format)
                                    .to_string()
                            })
                        })
                    })
                    .unwrap_or_default();
                if cache_ts.clone().is_some() {
                    widgets.cache_timestamp.set_visible(true);
                    widgets
                        .cache_timestamp
                        .set_label(&cache_ts.unwrap_or_default());
                } else {
                    widgets.cache_timestamp.set_visible(false);
                    widgets.cache_timestamp.set_label("");
                }
                widgets.btn_cache_toggle.set_active(toggled);
                widgets.pag_total_entry.set_text("");
                widgets.pag_current_entry.set_text("");
                widgets.pag_last_entry.set_text("");
                self.messages_wrapper.clear();
                self.page_size_combo.widget().queue_allocate();
                sender.input(MessagesTabMsg::ToggleMode(toggled));
            }
            MessagesTabMsg::LiveSearchMessages(term) => {
                match self.mode {
                    MessagesMode::Live => {
                        self.messages_wrapper.clear_filters();
                        let search_term = term.clone();
                        self.messages_wrapper
                            .add_filter(move |item| item.value.contains(search_term.as_str()));
                        let total = widgets.messages_view.model().unwrap().n_items();
                        info!("Total messages::{}", total);
                        fill_pagination(widgets, total as usize, 0);
                    }
                    MessagesMode::Cached { refresh: _ } => (),
                };
            }
            MessagesTabMsg::SearchMessages => {
                info!("[SearchMessages] {}", self.mode);
                match self.mode {
                    MessagesMode::Live => {
                        info!("[SearchMessages] Live mode, do nothing");
                    }
                    MessagesMode::Cached { refresh: _ } => {
                        sender.input(MessagesTabMsg::GetMessages)
                    }
                };
            }
            MessagesTabMsg::GotoPage => {
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::GetMessages => {
                info!("[GetMessages] {}", self.mode);
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
                let page: usize = widgets
                    .pag_current_entry
                    .text()
                    .to_string()
                    .parse()
                    .unwrap_or(1);
                let search_order = self.cache_search_order.clone();
                let search = get_search_term(widgets);
                let fetch = self.fetch_type.clone();
                let max_messages: i64 = self.max_messages as i64;
                widgets
                    .pag_current_entry
                    .set_text(page.to_string().as_str());
                let task_name = topic.name.clone();
                let task = Task::new(
                    TaskVariant::FetchMessages,
                    Some(task_name),
                    Some(self.token.clone()),
                );
                TOASTER_BROKER.send(AppMsg::ShowToast(task.id.clone(), "Working...".to_string()));
                TASK_MANAGER_BROKER.send(TaskManagerMsg::AddTask(task.clone()));
                let cache = self.cache_settings.clone();
                sender.oneshot_command(async move {
                    // Run async background task
                    let messages_worker = MessagesWorker::new();
                    let result = &messages_worker
                        .get_messages(&MessagesRequest {
                            task: Some(task),
                            mode,
                            connection: conn,
                            topic: topic.clone(),
                            page,
                            search_order,
                            page_size,
                            search,
                            fetch,
                            max_messages,
                            cache,
                        })
                        .await
                        .unwrap();
                    let total = result.total;
                    trace!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::Data(result.clone())
                });
            }
            MessagesTabMsg::GetNextMessages => {
                let page_size = self.page_size;
                let page: usize = widgets
                    .pag_current_entry
                    .text()
                    .to_string()
                    .parse()
                    .unwrap_or(1)
                    + 1;
                widgets
                    .pag_current_entry
                    .set_text(page.to_string().as_str());
                info!(
                    "getting next messages [page_size={}, page={}]",
                    page_size, page
                );
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::GetPreviousMessages => {
                let page_size = self.page_size;
                let page: usize = widgets
                    .pag_current_entry
                    .text()
                    .to_string()
                    .parse()
                    .unwrap_or(1)
                    - 1;
                widgets
                    .pag_current_entry
                    .set_text(page.to_string().as_str());
                info!(
                    "getting previous messages [page_size={}, page={}]",
                    page_size, page
                );
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::RefreshCache => {
                info!("refreshing cached messages");
                self.mode = MessagesMode::Cached { refresh: true };
                sender.input(MessagesTabMsg::GetMessages);
            }
            MessagesTabMsg::DestroyCache => {
                info!("destroying cached messages");
                let conn = self.connection.clone().unwrap();
                let topic = self.topic.clone().unwrap();
                let result_topic =
                    MessagesWorker::new().cleanup_messages(&MessagesCleanupRequest {
                        connection_id: conn.id.unwrap(),
                        topic_name: topic.name.clone(),
                        refresh: false,
                    });
                info!("destroying cached message::{:?}", result_topic.clone());
                match result_topic {
                    Some(_) => self.topic = result_topic,
                    None => warn!("unable to destroy cache"),
                };
                widgets.cache_timestamp.set_text("");
                widgets.cache_timestamp.set_visible(false);
                widgets.btn_cache_toggle.set_active(false);
                sender.input(MessagesTabMsg::ToggleMode(false));
            }
            MessagesTabMsg::RefreshTopic => {
                let conn = self.connection.clone().unwrap();
                let topic = self.topic.clone().unwrap();
                let result_topic = Repository::new().find_topic(conn.id.unwrap(), &topic.name);
                match result_topic {
                    Some(topic) => {
                        self.topic = Some(topic.clone());
                        if topic.cached.is_none() {
                            widgets.cache_timestamp.set_text("");
                            widgets.cache_timestamp.set_visible(false);
                            widgets.btn_cache_toggle.set_active(false);
                            sender.input(MessagesTabMsg::ToggleMode(false));
                        }
                    }
                    None => warn!("unable to refresh topic after cache destroyed"),
                };
            }
            MessagesTabMsg::RefreshTotalCounter => {
                info!("refreshing total counter");
                let task = Task::new(
                    TaskVariant::FetchMessages,
                    Some("refresh_total_counter".to_string()),
                    None,
                );
                TOASTER_BROKER.send(AppMsg::ShowToast(
                    task.id.clone(),
                    "Counting messages...".to_string(),
                ));
                let conn = self.connection.clone().unwrap();
                let topic = self.topic.clone().unwrap();
                sender.oneshot_command(async move {
                    // Run async background task
                    let messages_worker = MessagesWorker::new();
                    let total = messages_worker
                        .count_messages(&MessagesTotalCounterRequest {
                            connection: conn,
                            topic: topic.clone(),
                        })
                        .await
                        .unwrap_or_default();
                    debug!("selected topic {} with {} messages", topic.name, &total,);
                    CommandMsg::RefreshTotalCounterResult(task.id, total)
                });
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
                let settings = Settings::read().unwrap_or_default();
                let timestamp_formatter = settings.timestamp_formatter();
                let total = response.total;
                self.topic = response.topic.clone();
                self.cache_settings = self.topic.clone().and_then(|t| t.cached);
                match self.mode {
                    MessagesMode::Live => info!("no need to cleanup list on live mode"),
                    MessagesMode::Cached { refresh: _ } => self.messages_wrapper.clear(),
                }
                on_loading(widgets, true);
                fill_pagination(widgets, total, response.page_size);
                if self.mode == MessagesMode::Live {
                    let sort_column = settings.messages_sort_column;
                    let sort_column: Option<&ColumnViewColumn> = self
                        .messages_wrapper
                        .get_columns()
                        .get(sort_column.as_str());
                    let sort_order =
                        MessagesSortOrder::from_str(settings.messages_sort_column_order.as_str())
                            .unwrap_or_default();
                    let sort_type = match sort_order {
                        MessagesSortOrder::Ascending => gtk::SortType::Ascending,
                        MessagesSortOrder::Descending => gtk::SortType::Descending,
                        MessagesSortOrder::Default => match self.fetch_type {
                            KafkaFetch::Newest => gtk::SortType::Descending,
                            KafkaFetch::Oldest => gtk::SortType::Ascending,
                        },
                    };
                    info!(
                        "sort_column::{:?}, sort_type::{:?}",
                        sort_column.map(|c| c.title()),
                        sort_type
                    );
                    widgets.messages_view.sort_by_column(sort_column, sort_type);
                };

                self.messages_wrapper.extend_from_iter(
                    response
                        .messages
                        .iter()
                        .map(|m| MessageListItem::new(m.clone(), timestamp_formatter.clone())),
                );
                self.message_viewer.emit(MessageViewerMsg::Clear);
                let cache_ts = response.topic.and_then(|t| {
                    t.cached.map(|c| {
                        c.last_updated
                            .map(|ts| {
                                Utc.timestamp_millis_opt(ts)
                                    .unwrap()
                                    .with_timezone(&America::Sao_Paulo)
                                    .format(&timestamp_formatter)
                                    .to_string()
                            })
                            .unwrap_or_default()
                    })
                });
                if let Some(cache_ts) = cache_ts {
                    widgets.cache_timestamp.set_label(&cache_ts);
                    widgets.cache_timestamp.set_visible(true);
                } else {
                    widgets.cache_timestamp.set_label("");
                    widgets.cache_timestamp.set_visible(false);
                }
                TOASTER_BROKER.send(AppMsg::HideToast(response.task.clone().unwrap().id.clone()));
                STATUS_BROKER.send(StatusBarMsg::StopWithInfo {
                    text: Some(format!("{} messages loaded!", self.messages_wrapper.len())),
                });
            }
            MessagesTabMsg::OpenMessage(message_idx) => {
                let item = self.messages_wrapper.get_visible(message_idx).unwrap();
                let message_text = item.borrow().value.clone();
                let headers = item.borrow().headers.clone();
                self.message_viewer
                    .emit(MessageViewerMsg::Open(message_text, headers));
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
            CommandMsg::MessagesResendResult(task_id, result) => {
                if result.is_some() {
                    info!("messages resent!");
                } else {
                    let main_window = main_application().active_window().unwrap();
                    show_error_alert(&main_window, "Unable to send messages".to_string());
                }
                TOASTER_BROKER.send(AppMsg::HideToast(task_id));
            }
            CommandMsg::Data(messages) => {
                sender.input(MessagesTabMsg::UpdateMessages(Box::new(messages)))
            }
            CommandMsg::CopyToClipboard(id, data) => {
                let data_size = format_size(data.len(), DECIMAL);
                info!("setting text to clipboard: {}", data_size);
                self.clipboard.set_contents(data).unwrap_or_else(|err| {
                    warn!("Unable to store text in clipboard: {}", err);
                });
                TOASTER_BROKER.send(AppMsg::HideToast(id));
            }
            CommandMsg::RefreshTotalCounterResult(id, total) => {
                widgets
                    .total_counter_entry
                    .set_text(total.to_string().as_str());
                TOASTER_BROKER.send(AppMsg::HideToast(id));
            }
        }
    }
}

impl MessagesTabModel {
    fn find_cache(&mut self) -> Option<KrustTopicCache> {
        let connection_id = self
            .connection
            .clone()
            .expect("should have connection")
            .id
            .unwrap();
        let topic_name = &self.topic.clone().expect("should have a topic").name;
        let mut repo = Repository::new();
        repo.find_topic_cache(connection_id, topic_name)
    }
}

fn get_search_term(widgets: &mut MessagesTabModelWidgets) -> Option<String> {
    let search: Option<String> = Some(widgets.messages_search_entry.text().into());
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
    widgets.btn_cache_destroy.set_sensitive(enabled);
    widgets.btn_cache_toggle.set_sensitive(enabled);
    widgets.max_messages.set_sensitive(enabled);
}

fn fill_pagination(widgets: &mut MessagesTabModelWidgets, total: usize, page_size: u16) {
    let current_page: usize = widgets
        .pag_current_entry
        .text()
        .to_string()
        .parse::<usize>()
        .unwrap_or_default();

    widgets.pag_total_entry.set_text(total.to_string().as_str());
    widgets
        .pag_current_entry
        .set_text(current_page.to_string().as_str());
    let pages = ((total as f64) / (page_size as f64)).ceil() as usize;
    widgets.pag_last_entry.set_text(pages.to_string().as_str());

    info!(
        "fill pagination of current page {} of {}",
        current_page, pages
    );
    match current_page {
        1 if pages == 1 => {
            widgets.btn_next_page.set_sensitive(false);
            widgets.btn_previous_page.set_sensitive(false);
        }
        1 => {
            widgets.btn_next_page.set_sensitive(true);
            widgets.btn_previous_page.set_sensitive(false);
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
    let timestamp_format = Settings::read().unwrap_or_default().timestamp_formatter();
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b';')
        .quote_style(csv::QuoteStyle::NonNumeric)
        .from_writer(vec![]);
    let _ = wtr.write_record(["PARTITION", "OFFSET", "KEY", "VALUE", "TIMESTAMP"]);
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
        let timestamp = Utc
            .timestamp_millis_opt(timestamp.unwrap_or_default())
            .unwrap()
            .with_timezone(&America::Sao_Paulo)
            .format(&timestamp_format)
            .to_string();
        let record = StringRecord::from(vec![
            partition.to_string(),
            offset.to_string(),
            key.unwrap_or_default(),
            clean_value,
            timestamp,
        ]);
        let _ = wtr.write_record(&record);
    }
    String::from_utf8(wtr.into_inner().unwrap_or_default())
}
fn copy_key_value(
    selected_items: &Vec<KrustMessage>,
) -> Result<String, std::string::FromUtf8Error> {
    let mut copy_content = String::default();
    for item in selected_items {
        let key = item.key.clone();
        let value = item.value.clone();
        let clean_value = match serde_json::from_str::<serde_json::Value>(value.as_str()) {
            Ok(json) => json.to_string(),
            Err(_) => value.replace('\n', ""),
        };
        let copy_text = format!("{},{}\n", key.unwrap_or_default(), clean_value);
        copy_content.push_str(copy_text.as_str());
    }
    Ok(copy_content)
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
        copy_content.push_str(copy_text.as_str());
    }
    Ok(copy_content)
}
fn copy_key(selected_items: &Vec<KrustMessage>) -> Result<String, std::string::FromUtf8Error> {
    let mut copy_content = String::default();
    for item in selected_items {
        let key = item.key.clone();
        let copy_text = format!("{}\n", key.unwrap_or_default());
        copy_content.push_str(copy_text.as_str());
    }
    Ok(copy_content)
}
