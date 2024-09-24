use core::f64;
use std::str::FromStr;

use adw::{prelude::*, AlertDialog};
use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::America;
use copypasta::{ClipboardContext, ClipboardProvider};
use gtk::Adjustment;
use relm4::*;
use relm4_components::simple_adw_combo_row::SimpleComboRow;
use tracing::*;
use uuid::Uuid;

use crate::backend::kafka::{KafkaBackend, KafkaFetch};
use crate::backend::repository::{FetchMode, KrustConnection, KrustTopic, KrustTopicCache};
use crate::backend::worker::{MessagesCleanupRequest, MessagesWorker};
use crate::component::messages::messages_tab::AVAILABLE_PAGE_SIZES;
use crate::modals::utils::build_confirmation_alert;
use crate::{AppMsg, Repository, DATE_TIME_FORMAT, TOASTER_BROKER};

const DEFAULT_MESSAGES_PER_PARTITION: usize = 10000;

pub struct MessagesCacheSettingsDialogModel {
    pub connection: KrustConnection,
    pub topic: KrustTopic,
    pub default_page_size_combo: Controller<SimpleComboRow<u16>>,
    pub selected_fetch_mode: Option<FetchMode>,
    pub selected_default_page_size: Option<u16>,
    pub confirmation_alert: AlertDialog,
    pub clipboard: Box<dyn ClipboardProvider>,
}

#[derive(Debug)]
pub enum MessagesCacheSettingsDialogMsg {
    Show,
    DefaultPageSizeSelected(usize),
    FetchModeSelected(FetchMode),
    ApplySettings,
    ConfirmApplySettings,
    Ignore,
    RefreshTopicMessagesCounter,
    CopyToClipboard(String),
}

#[derive(Debug)]
pub enum MessagesCacheSettingsDialogOutput {
    Update(KrustTopicCache),
}

#[derive(Debug)]
pub enum AsyncCommandOutput {
    RefreshTopicMessagesCounter(KrustTopic),
}

#[relm4::component(pub)]
impl Component for MessagesCacheSettingsDialogModel {
    type Init = (KrustConnection, KrustTopic);
    type Input = MessagesCacheSettingsDialogMsg;
    type Output = MessagesCacheSettingsDialogOutput;
    type CommandOutput = AsyncCommandOutput;

    view! {
        #[root]
        main_dialog = adw::Dialog {
            set_title: "Cache",
            set_content_width: 730,
            set_content_height: 350,
            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                adw::HeaderBar {},
                set_valign: gtk::Align::Fill,
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_valign: gtk::Align::Fill,
                    set_margin_all: 10,
                    adw::PreferencesGroup {
                        set_title: "Status",
                        #[name(status_topic_name)]
                        adw::ActionRow {
                            set_subtitle: "Topic name",
                            add_suffix = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                gtk::Button {
                                    set_tooltip_text: Some("Copy value to clipboard"),
                                    set_icon_name: "edit-copy-symbolic",
                                    set_margin_start: 5,
                                    set_valign: gtk::Align::Center,
                                    connect_clicked[sender,status_topic_name] => move |_| {
                                        let text = status_topic_name.title().to_string();
                                        sender.input(MessagesCacheSettingsDialogMsg::CopyToClipboard(text));
                                    },
                                },
                            },
                        },
                        #[name(status_topic_partitions)]
                        adw::ActionRow {
                            set_subtitle: "Topic partitions",
                            add_suffix = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                gtk::Button {
                                    set_tooltip_text: Some("Refresh value"),
                                    set_icon_name: "media-playlist-repeat-symbolic",
                                    set_margin_start: 5,
                                    set_valign: gtk::Align::Center,
                                    connect_clicked[sender] => move |_| {
                                        sender.input(MessagesCacheSettingsDialogMsg::RefreshTopicMessagesCounter);
                                    },
                                },
                                gtk::Button {
                                    set_tooltip_text: Some("Copy value to clipboard"),
                                    set_icon_name: "edit-copy-symbolic",
                                    set_margin_start: 5,
                                    set_valign: gtk::Align::Center,
                                    connect_clicked[sender,status_topic_partitions] => move |_| {
                                        let text = status_topic_partitions.title().to_string();
                                        sender.input(MessagesCacheSettingsDialogMsg::CopyToClipboard(text));
                                    },
                                },
                            },
                        },
                        #[name(status_topic_messages_count)]
                        adw::ActionRow {
                            set_subtitle: "Topic messages count",
                            add_suffix = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                gtk::Button {
                                    set_tooltip_text: Some("Refresh value"),
                                    set_icon_name: "media-playlist-repeat-symbolic",
                                    set_margin_start: 5,
                                    set_valign: gtk::Align::Center,
                                    connect_clicked[sender] => move |_| {
                                        sender.input(MessagesCacheSettingsDialogMsg::RefreshTopicMessagesCounter);
                                    },
                                },
                                gtk::Button {
                                    set_tooltip_text: Some("Copy value to clipboard"),
                                    set_icon_name: "edit-copy-symbolic",
                                    set_margin_start: 5,
                                    set_valign: gtk::Align::Center,
                                    connect_clicked[sender,status_topic_messages_count] => move |_| {
                                        let text = status_topic_messages_count.title().to_string();
                                        sender.input(MessagesCacheSettingsDialogMsg::CopyToClipboard(text));
                                    },
                                },
                            },
                        },
                    },
                    adw::PreferencesGroup {
                        set_title: "Settings",
                        #[local_ref]
                        default_page_size_combo -> adw::ComboRow {
                            set_title: "Default page size",
                            set_subtitle: "Set default page size for cache pagination",
                            set_visible: true,
                        },
                        adw::ActionRow {
                            set_title: "Fetch mode",
                            set_css_classes: &["fetch-mode"],
                            add_suffix = &gtk::StackSwitcher {
                                set_overflow: gtk::Overflow::Hidden,
                                set_orientation: gtk::Orientation::Horizontal,
                                set_stack: Some(&fetch_mode_stack),
                                set_hexpand: true,
                                set_vexpand: true,
                                set_valign: gtk::Align::Fill,
                                set_halign: gtk::Align::Fill,
                            },
                        },

                        #[name(fetch_mode_stack)]
                        gtk::Stack {
                            set_hhomogeneous: true,
                            add_child = &gtk::Box {
                                set_halign: gtk::Align::Fill,
                                set_hexpand: true,
                                set_orientation: gtk::Orientation::Vertical,
                                gtk::Label {
                                    set_label: "All messages",
                                    set_halign: gtk::Align::Start,
                                    set_margin_top: 8,
                                    set_margin_start: 10,
                                },
                            } -> {
                                set_title: FetchMode::All.to_string().as_str(),
                                set_name: FetchMode::All.to_string().as_str(),
                            },
                            add_child: first_n_messages = &adw::SpinRow {
                                    set_title: "Messages per partition",
                                    set_subtitle: "First (n) messages per partitions",
                                    set_valign: gtk::Align::Start,
                                    set_numeric: true,
                                    set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                                    set_adjustment = Some(&offset_adjustment),
                            } -> {
                                set_title: FetchMode::Head.to_string().as_str(),
                                set_name: FetchMode::Head.to_string().as_str(),
                            },
                            add_child: last_n_messages = &adw::SpinRow {
                                    set_title: "Messages per partition",
                                    set_subtitle: "Last (n) messages per partitions",
                                    set_valign: gtk::Align::Start,
                                    set_numeric: true,
                                    set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                                    set_adjustment = Some(&offset_adjustment),
                            } -> {
                                set_title: FetchMode::Tail.to_string().as_str(),
                                set_name: FetchMode::Tail.to_string().as_str(),
                            },
                            add_child: timestamp_widget = &adw::ActionRow {
                                    set_title: "From offset date/time",
                                    set_valign: gtk::Align::Start,
                                    set_margin_top: 4,
                                    add_suffix: timestamp_container = &gtk::Box {
                                        set_orientation: gtk::Orientation::Horizontal,
                                        #[name(formatted_date)]
                                        gtk::Entry {
                                            set_valign: gtk::Align::Center,
                                            set_editable: false,
                                            set_max_width_chars: 10,
                                        },
                                        gtk::MenuButton {
                                            set_tooltip_text: Some("Show calendar"),
                                            set_valign: gtk::Align::Center,
                                            set_direction: gtk::ArrowType::Down,
                                            add_css_class: "flat",
                                            set_margin_end: 5,
                                            #[wrap(Some)]
                                            set_popover: calendar_popover = &gtk::Popover {
                                                set_position: gtk::PositionType::Bottom,
                                                #[wrap(Some)]
                                                set_child: calendar = &gtk::Calendar {
                                                    connect_day_selected[formatted_date] => move |calendar| {
                                                        let date = calendar.date();
                                                        let year = date.year();
                                                        let month = date.month();
                                                        let day = date.day_of_month();
                                                        let date_fmt = format!("{:02}/{:02}/{}", day, month, year);
                                                        formatted_date.set_text(date_fmt.as_str());
                                                    },
                                                },
                                            },
                                        },
                                        #[name(time_hours)]
                                        gtk::SpinButton {
                                            set_xalign: 0.5,
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_wrap: true,
                                            set_numeric: true,
                                            set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                                            set_increments: (1.0, 1.0),
                                            set_range: (0.0, 23.0),
                                            set_digits: 0,
                                        },
                                        gtk::Label { set_label: ":", set_margin_start: 2, set_margin_end: 2, },
                                        #[name(time_minutes)]
                                        gtk::SpinButton {
                                            set_xalign: 0.5,
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_wrap: true,
                                            set_numeric: true,
                                            set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                                            set_increments: (1.0, 1.0),
                                            set_range: (0.0, 59.0),
                                            set_digits: 0,
                                        },
                                        gtk::Label { set_label: ":", set_margin_start: 2, set_margin_end: 2, },
                                        #[name(time_seconds)]
                                        gtk::SpinButton {
                                            set_xalign: 0.5,
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_wrap: true,
                                            set_numeric: true,
                                            set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                                            set_increments: (1.0, 1.0),
                                            set_range: (0.0, 59.0),
                                            set_digits: 0,
                                        },
                                    },
                            } -> {
                                set_title: FetchMode::FromTimestamp.to_string().as_str(),
                                set_name: FetchMode::FromTimestamp.to_string().as_str(),
                            },
                            connect_visible_child_name_notify[sender] => move |stack| {
                                let selected = stack.visible_child_name();
                                if let Some(selected) = selected {
                                    let fetch_mode = FetchMode::from_str(selected.to_string().as_str());
                                    sender.input(MessagesCacheSettingsDialogMsg::FetchModeSelected(fetch_mode.unwrap_or_default()));
                                };
                            },
                        },
                        gtk::Button {
                            set_label: "Apply",
                            add_css_class: "suggested-action",
                            set_margin_top: 4,
                            connect_clicked => MessagesCacheSettingsDialogMsg::ApplySettings,
                        },
                    }
                }
            }
        }
    }

    fn init(
        current_connection: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (connection, topic) = current_connection.clone();
        let default_idx = 0;

        let default_page_size_combo = SimpleComboRow::builder()
            .launch(SimpleComboRow {
                variants: AVAILABLE_PAGE_SIZES.to_vec(),
                active_index: Some(default_idx),
            })
            .forward(
                sender.input_sender(),
                MessagesCacheSettingsDialogMsg::DefaultPageSizeSelected,
            );
        let confirmation_alert = build_confirmation_alert(
            "Apply".to_string(),
            "Are you sure you want to delete the topic cache and apply the new settings?"
                .to_string(),
        );
        let snd: ComponentSender<MessagesCacheSettingsDialogModel> = sender.clone();
        confirmation_alert.connect_response(Some("cancel"), move |_, _| {
            snd.input(MessagesCacheSettingsDialogMsg::Ignore);
        });
        let snd: ComponentSender<MessagesCacheSettingsDialogModel> = sender.clone();
        confirmation_alert.connect_response(Some("confirm"), move |_, _| {
            snd.input(MessagesCacheSettingsDialogMsg::ConfirmApplySettings);
        });
        let clipboard = Box::new(ClipboardContext::new().unwrap());
        info!("init::[connection={:?}, topic={:?}]", &connection, &topic);
        let model = MessagesCacheSettingsDialogModel {
            connection,
            topic,
            default_page_size_combo,
            selected_fetch_mode: None,
            selected_default_page_size: None,
            confirmation_alert,
            clipboard,
        };
        let default_page_size_combo = model.default_page_size_combo.widget();
        let offset_adjustment = Adjustment::builder()
            .lower(1.0)
            .upper(i32::MAX as f64)
            .page_size(1000.0)
            .step_increment(1.0)
            .value(DEFAULT_MESSAGES_PER_PARTITION as f64)
            .build();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: MessagesCacheSettingsDialogMsg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        debug!("received message: {:?}", msg);

        match msg {
            MessagesCacheSettingsDialogMsg::Show => {
                let connection = self.connection.clone();
                let conn_id = connection.id.unwrap();
                let mut repository = Repository::new();

                widgets.status_topic_name.set_title(&self.topic.name);

                let cached = repository.find_topic_cache(conn_id, &self.topic.name);

                let default_page_size_idx = cached
                    .clone()
                    .map(|c| c.default_page_size as u32)
                    .unwrap_or_default();
                let fetch_mode = cached.clone().map(|c| c.fetch_mode).unwrap_or_default();
                let fetch_value = cached
                    .clone()
                    .and_then(|c| c.fetch_value)
                    .unwrap_or_default();
                info!(
                    "settings fetch mode {}, with default page size index::{}",
                    fetch_mode.to_string(),
                    default_page_size_idx
                );

                match fetch_mode {
                    FetchMode::Head => {
                        widgets.first_n_messages.set_value(fetch_value as f64);
                    }
                    FetchMode::Tail => {
                        widgets.last_n_messages.set_value(fetch_value as f64);
                    }
                    FetchMode::All => (),
                    FetchMode::FromTimestamp => {
                        let date_time = Utc
                            .timestamp_millis_opt(fetch_value)
                            .unwrap()
                            .with_timezone(&America::Sao_Paulo);
                        let day = date_time.day();
                        let month = date_time.month();
                        let year = date_time.year();
                        widgets.calendar.set_day(day as i32);
                        widgets.calendar.set_month(month as i32);
                        widgets.calendar.set_year(year);
                        let hours = date_time.hour();
                        let minutes = date_time.minute();
                        let seconds = date_time.second();
                        widgets.time_hours.set_value(hours as f64);
                        widgets.time_minutes.set_value(minutes as f64);
                        widgets.time_seconds.set_value(seconds as f64);
                    }
                };
                widgets
                    .default_page_size_combo
                    .set_selected(default_page_size_idx);
                widgets
                    .fetch_mode_stack
                    .set_visible_child_name(&fetch_mode.to_string());
                sender.input(MessagesCacheSettingsDialogMsg::RefreshTopicMessagesCounter);

                let parent = &relm4::main_application().active_window().unwrap();
                root.queue_allocate();
                root.present(parent);
            }
            MessagesCacheSettingsDialogMsg::RefreshTopicMessagesCounter => {
                let connection = self.connection.clone();
                let topic = self.topic.clone();
                let topic_name = topic.name.clone();
                let kafka = KafkaBackend::new(&connection);

                sender.oneshot_command(async move {
                    let topic = kafka
                        .topic_message_count(&topic_name, Some(KafkaFetch::Oldest), None, None)
                        .await;
                    AsyncCommandOutput::RefreshTopicMessagesCounter(topic)
                });
            }
            MessagesCacheSettingsDialogMsg::CopyToClipboard(text) => {
                let id = Uuid::new_v4();
                TOASTER_BROKER.send(AppMsg::ShowToast(id.to_string(), "Copied!".to_string()));
                self.clipboard.set_contents(text).unwrap_or_else(|err| {
                    warn!("Unable to store text in clipboard: {}", err);
                });
                TOASTER_BROKER.send(AppMsg::HideToast(id.to_string()));
            }
            MessagesCacheSettingsDialogMsg::DefaultPageSizeSelected(index) => {
                self.selected_default_page_size = Some(index as u16);
            }
            MessagesCacheSettingsDialogMsg::FetchModeSelected(mode) => {
                info!("Fetch mode selected: {:?}", mode);
                self.selected_fetch_mode = Some(mode);
            }
            MessagesCacheSettingsDialogMsg::ApplySettings => {
                info!(
                    "Applying settings[connection={:?}, topic={:?}]...",
                    &self.connection, &self.topic
                );
                let mut repo = Repository::new();
                let connection = self.connection.clone();
                let conn_id = connection.id.unwrap();
                let topic = self.topic.clone();
                let topic_name = &topic.name;
                info!(
                    "getting updated topic[connection_id={}, topic_name={}]",
                    &conn_id, &topic_name
                );
                let updated_cache = repo.find_topic_cache(conn_id, topic_name);
                if let Some(cache) = updated_cache.clone() {
                    debug!("already has cache, asking for confirmation::{:?}", &cache);
                    self.confirmation_alert.present(&widgets.main_dialog);
                } else {
                    sender.input(MessagesCacheSettingsDialogMsg::ConfirmApplySettings);
                }
            }
            MessagesCacheSettingsDialogMsg::ConfirmApplySettings => {
                let worker = MessagesWorker::new();
                let connection_id = self.connection.id.unwrap();
                let topic_name = self.topic.name.clone();
                info!(
                    "Confirm settings[connection={:?}, topic={:?}]...",
                    &connection_id, &topic_name
                );
                worker.cleanup_messages(&MessagesCleanupRequest {
                    connection_id,
                    topic_name: topic_name.clone(),
                    refresh: false,
                });
                let fetch_value = match self.selected_fetch_mode {
                    Some(FetchMode::FromTimestamp) => {
                        let date_text = widgets.formatted_date.text().to_string();
                        let hours = widgets.time_hours.value_as_int();
                        let minutes = widgets.time_minutes.value_as_int();
                        let seconds = widgets.time_seconds.value_as_int();
                        let date_time_formatted =
                            format!("{} {:02}:{:02}:{:02}", date_text, hours, minutes, seconds);
                        let date_time = NaiveDateTime::parse_from_str(
                            date_time_formatted.as_str(),
                            DATE_TIME_FORMAT,
                        );
                        let now = Local::now();
                        let offset = now.offset();
                        let date_time = date_time.unwrap().and_local_timezone(*offset).unwrap();
                        info!("date_time::{:?}", date_time);
                        info!("date_time::utc::{}", date_time.timestamp_millis());
                        Some(date_time.timestamp_millis())
                    }
                    Some(FetchMode::Head) => Some(widgets.first_n_messages.value() as i64),
                    Some(FetchMode::Tail) => Some(widgets.last_n_messages.value() as i64),
                    _ => None,
                };
                let default_page_size = self.selected_default_page_size.unwrap_or_default();
                let cache = KrustTopicCache {
                    connection_id,
                    topic_name: topic_name.clone(),
                    fetch_mode: self.selected_fetch_mode.unwrap_or_default(),
                    fetch_value,
                    default_page_size,
                    last_updated: Some(Utc::now().timestamp_millis()),
                };
                let result = sender.output(MessagesCacheSettingsDialogOutput::Update(cache));
                match result {
                    Ok(_) => info!("cache settings output sent!"),
                    Err(e) => error!("cache settings output error: {:?}", e),
                }
                root.close();
            }
            MessagesCacheSettingsDialogMsg::Ignore => {
                info!("Ignore settings...");
            }
        };

        self.update_view(widgets, sender);
    }
    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AsyncCommandOutput::RefreshTopicMessagesCounter(topic) => {
                widgets
                    .status_topic_partitions
                    .set_title(&topic.partitions.len().to_string());
                widgets
                    .status_topic_messages_count
                    .set_title(&topic.total.unwrap_or_default().to_string());
            }
        }
    }
}
