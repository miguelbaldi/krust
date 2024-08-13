use adw::prelude::*;
use gtk::gdk::DisplayManager;
use relm4::*;
use relm4_components::simple_adw_combo_row::{SimpleComboRow, SimpleComboRowMsg};
use tracing::*;

use crate::backend::{
    kafka::KafkaBackend,
    repository::{KrustConnection, KrustMessage, KrustTopic},
};

#[derive(Debug, Clone, Copy, Default)]
pub enum MultiFormat {
    Key,
    Value,
    #[default]
    KeyValue,
}

impl MultiFormat {
    fn all() -> Vec<Self> {
        vec![Self::KeyValue, Self::Key, Self::Value]
    }
}

impl ToString for MultiFormat {
    fn to_string(&self) -> String {
        match self {
            Self::Key => "Key".to_string(),
            Self::Value => "Value".to_string(),
            Self::KeyValue => "Key and Value".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct MessagesSendDialogModel {
    pub connection: Option<KrustConnection>,
    pub topic: Option<KrustTopic>,
    pub partitions_combo: Controller<SimpleComboRow<String>>,
    pub selected_partition: Option<i32>,
    pub multi_format_combo: Controller<SimpleComboRow<MultiFormat>>,
    pub selected_multi_format: Option<MultiFormat>,
    pub is_multiple: bool,
}

#[derive(Debug)]
pub enum MessagesSendDialogMsg {
    Show,
    PartitionSelected(usize),
    LoadPartitions,
    ToggleMultipleMessages(bool),
    MultiFormatSelected(usize),
    Cancel,
    Send,
    RecalculateDialogSize,
}

#[derive(Debug)]
pub enum AsyncCommandOutput {
    SetPartitions(Vec<String>),
    SendResult,
}

#[relm4::component(pub)]
impl Component for MessagesSendDialogModel {
    type Init = (Option<KrustConnection>, Option<KrustTopic>);
    type Input = MessagesSendDialogMsg;
    type Output = ();
    type CommandOutput = AsyncCommandOutput;

    view! {
        #[root]
        main_dialog = adw::Dialog {
            set_title: "Add messages",
            // set_content_width: dialog_width,
            // set_content_height: dialog_height,
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
                        set_title: "Settings",
                        #[local_ref]
                        partitions_combo -> adw::ComboRow {
                            set_title: "Partition",
                            set_subtitle: "Select target topic partition",
                            set_use_subtitle: true,
                        },
                        #[name(toggle_multiple_messages)]
                        adw::SwitchRow {
                            set_title: "Multiple",
                            set_subtitle: "Enable multiple messages mode",
                            connect_active_notify[sender] => move |b| {
                                sender.input(MessagesSendDialogMsg::ToggleMultipleMessages(b.is_active()));
                            },
                        },
                        #[local_ref]
                        multi_format_combo -> adw::ComboRow {
                            set_title: "Format",
                            set_subtitle: "Each line contains",
                            set_visible: false,
                        },
                        #[name(multiple_key_value_separator)]
                        adw::EntryRow {
                            set_title: "Key/Value separator",
                            set_show_apply_button: true,
                            set_text: ",",
                            set_visible: false,
                        }
                    },
                    #[name(single_message_key_group)]
                    adw::PreferencesGroup {
                        set_title: "Key",
                        set_margin_top: 10,
                        set_vexpand: false,
                        set_hexpand: true,
                        #[name(single_message_key_container)]
                        gtk::ScrolledWindow {
                            set_vexpand: false,
                            set_hexpand: true,
                            set_propagate_natural_height: true,
                            set_overflow: gtk::Overflow::Hidden,
                            set_valign: gtk::Align::Start,
                            add_css_class: "entry",
                            #[name(single_message_key)]
                            gtk::TextView {
                                set_top_margin: 5,
                                set_left_margin: 5,
                                set_monospace: true,
                                add_css_class: "message-textview",
                            },
                        },
                    },
                    #[name(single_message_value_group)]
                    adw::PreferencesGroup {
                        set_title: "Message",
                        set_margin_top: 10,
                        set_vexpand: true,
                        set_hexpand: true,
                        set_valign: gtk::Align::BaselineFill,
                        add_css_class: "message-group",
                        #[name(single_message_value_container)]
                        gtk::ScrolledWindow {
                            set_vexpand: true,
                            set_hexpand: true,
                            set_propagate_natural_height: true,
                            set_overflow: gtk::Overflow::Hidden,
                            set_valign: gtk::Align::Fill,
                            add_css_class: "entry",
                            #[name(single_message_value)]
                            gtk::TextView {
                                set_valign: gtk::Align::Fill,
                                set_vexpand: true,
                                set_monospace: true,
                                set_top_margin: 5,
                                set_left_margin: 5,
                                add_css_class: "message-textview",
                            },
                        },
                    },
                    gtk::Box {
                        set_margin_top: 10,
                        set_margin_bottom: 10,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::End,
                        #[name(single_message_send)]
                        gtk::Button {
                            set_label: "Send",
                            add_css_class: "destructive-action",
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesSendDialogMsg::Send);
                            },
                        },
                        #[name(single_message_cancel)]
                        gtk::Button {
                            set_label: "Cancel",
                            set_margin_start: 10,
                            connect_clicked[sender] => move |_| {
                                sender.input(MessagesSendDialogMsg::Cancel);
                            },
                        },
                    }
                },
            },
        }
    }

    fn init(
        current_connection: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (connection, topic) = current_connection.clone();
        let default_idx = 0;
        let partitions_combo = SimpleComboRow::builder()
            .launch(SimpleComboRow {
                variants: vec!["0".to_string(), "1".to_string()],
                active_index: Some(default_idx),
            })
            .forward(
                sender.input_sender(),
                MessagesSendDialogMsg::PartitionSelected,
            );
        let multi_format_combo = SimpleComboRow::builder()
            .launch(SimpleComboRow {
                variants: MultiFormat::all(),
                active_index: Some(default_idx),
            })
            .forward(
                sender.input_sender(),
                MessagesSendDialogMsg::MultiFormatSelected,
            );

        let model = MessagesSendDialogModel {
            connection: connection,
            topic: topic,
            partitions_combo: partitions_combo,
            selected_partition: None,
            multi_format_combo: multi_format_combo,
            selected_multi_format: None,
            is_multiple: false,
        };
        let partitions_combo = model.partitions_combo.widget();
        let multi_format_combo = model.multi_format_combo.widget();

        let window = &relm4::main_application().active_window().unwrap();
        // When the window is maximised or tiled
        let connect_sender = sender.clone();
        window.connect_maximized_notify(move |window| {
            let width = window.width();
            let height = window.height();
            info!("window_maximized_notify::{}x{}", width, height);
            connect_sender.input(MessagesSendDialogMsg::RecalculateDialogSize);
        });
        let connect_sender = sender.clone();
        window.connect_fullscreened_notify(move |window| {
            let width = window.width();
            let height = window.height();
            info!("window_fullscreened_notify::{}x{}", width, height);
            connect_sender.input(MessagesSendDialogMsg::RecalculateDialogSize);
        });
        // When the user manually drags the border of the window
        let connect_sender = sender.clone();
        window.connect_default_height_notify(move |window| {
            let width = window.width();
            let height = window.height();
            info!("default_height_notify::{}x{}", width, height);
            connect_sender.input(MessagesSendDialogMsg::RecalculateDialogSize);
        });
        let widgets = view_output!();
        sender.input(MessagesSendDialogMsg::LoadPartitions);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: MessagesSendDialogMsg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        info!("received message: {:?}", msg);

        match msg {
            MessagesSendDialogMsg::RecalculateDialogSize => {
                let (dialog_width, dialog_height) =
                    MessagesSendDialogModel::get_dialog_max_geometry();
                root.set_content_height(dialog_height);
                root.set_content_width(dialog_width);
                root.queue_allocate();
            }
            MessagesSendDialogMsg::Show => {
                let parent = &relm4::main_application().active_window().unwrap();
                let (dialog_width, dialog_height) =
                    MessagesSendDialogModel::get_dialog_max_geometry();
                root.set_content_height(dialog_height);
                root.set_content_width(dialog_width);
                root.queue_allocate();
                root.present(parent);
            }
            MessagesSendDialogMsg::Cancel => {
                root.close();
            }
            MessagesSendDialogMsg::Send => {
                if self.is_multiple {
                    self.send_multiple_message(widgets, sender.clone());
                } else {
                    self.send_single_message(widgets, sender.clone());
                }
            }
            MessagesSendDialogMsg::LoadPartitions => {
                let connection = self.connection.clone().unwrap();
                let topic_name = self.topic.clone().unwrap().name;
                sender.oneshot_command(async move {
                    // Run async background task
                    let kafka = KafkaBackend::new(&connection);
                    let result = &kafka.fetch_partitions(&topic_name).await;
                    let partitions = result.iter().map(|p| p.id.to_string()).collect();
                    trace!("partitions for topic {}: {:?}", &topic_name, &result,);
                    AsyncCommandOutput::SetPartitions(partitions)
                });
            }
            MessagesSendDialogMsg::PartitionSelected(_index) => {
                let partition_id = match self.partitions_combo.model().get_active_elem() {
                    Some(opt) => opt.clone().parse::<i32>().unwrap_or_default(),
                    None => 0,
                };
                info!("selected partition {}", partition_id);
                self.selected_partition = Some(partition_id);
            }
            MessagesSendDialogMsg::ToggleMultipleMessages(is_active) => {
                self.is_multiple = is_active;
                widgets.multi_format_combo.set_visible(is_active);
                widgets.multiple_key_value_separator.set_visible(is_active);
                widgets.single_message_key_group.set_visible(!is_active);
                if is_active {
                    widgets
                        .single_message_value_group
                        .set_title("Messages in the area below, one key and/or value per line");
                } else {
                    widgets.single_message_value_group.set_title("Message");
                }
            }
            MessagesSendDialogMsg::MultiFormatSelected(_index) => {
                let selected_format = self
                    .multi_format_combo
                    .model()
                    .get_active_elem()
                    .unwrap_or(&MultiFormat::default())
                    .clone();
                self.selected_multi_format = Some(selected_format);
            }
        };

        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AsyncCommandOutput::SetPartitions(partitions) => {
                let variants = partitions.clone();
                self.partitions_combo
                    .emit(SimpleComboRowMsg::UpdateData(SimpleComboRow {
                        variants: variants,
                        active_index: Some(0),
                    }));
            }
            AsyncCommandOutput::SendResult => {
                info!("SendResult");
                widgets.single_message_key.buffer().set_text("");
                widgets.single_message_value.buffer().set_text("");
                root.close();
            }
        }
    }
}

impl MessagesSendDialogModel {
    fn get_dialog_max_geometry() -> (i32, i32) {
        let (w_width, w_height) = MessagesSendDialogModel::get_display_resolution();
        info!(
            "get_dialog_max_geometry::dialog::window::{}x{}",
            w_width, w_height
        );
        let height = ((w_height as f32) * 0.9).ceil() as i32;
        let width = ((w_width as f32) * 0.9).ceil() as i32;

        info!("get_dialog_max_geometry::result::{}x{}", width, height);
        (width, height)
    }
    fn get_display_resolution() -> (i32, i32) {
        let default_geometry = (1024, 768);
        let main_window = main_application().active_window().unwrap();
        let surface = main_window.surface();
        let resolution_based = if let Some(surface) = surface {
            //let toplevel =  surface.downcast::<gtk::gdk::Toplevel>();
            if let Some(display) = DisplayManager::get().default_display() {
                if let Some(monitor) = display.monitor_at_surface(&surface) {
                    let height = monitor.geometry().height();
                    let width = monitor.geometry().width();
                    info!(
                        "get_display_resolution::monitor::resolution::{}x{}",
                        width, height
                    );
                    (width, height)
                } else {
                    default_geometry
                }
            } else {
                default_geometry
            }
        } else {
            default_geometry
        };

        info!(
            "get_display_resolution::result::{}x{}",
            resolution_based.0, resolution_based.1
        );
        resolution_based
    }

    fn send_multiple_message(
        &mut self,
        widgets: &mut MessagesSendDialogModelWidgets,
        sender: ComponentSender<Self>,
    ) {
        let selected_multi_format: MultiFormat = self.selected_multi_format.unwrap_or_else(|| {
            self.multi_format_combo
                .model()
                .get_active_elem()
                .unwrap_or(&MultiFormat::default())
                .clone()
        });
        info!("send_multiple_message::{:?}", self.selected_multi_format);
        let partition = self.selected_partition.unwrap_or(0);
        let topic = self.topic.clone().unwrap().name;
        let messages = match selected_multi_format {
            MultiFormat::Key => self
                .get_key(widgets, true)
                .iter()
                .map(|k| (k.clone(), String::default()))
                .collect(),
            MultiFormat::Value => self
                .get_value(widgets, true)
                .iter()
                .map(|v| (String::default(), v.clone()))
                .collect(),
            MultiFormat::KeyValue => self.get_key_value(widgets, true),
        };
        let messages: Vec<KrustMessage> = messages
            .iter()
            .map(|m| KrustMessage {
                topic: topic.clone(),
                partition,
                offset: 0,
                key: Some(m.0.clone()),
                value: m.1.clone(),
                timestamp: None,
                headers: vec![],
            })
            .collect();
        debug!("sending messages::{:?}", &messages);
        let connection = self.connection.clone().unwrap();
        sender.oneshot_command(async move {
            // Run async background task
            let kafka = KafkaBackend::new(&connection);
            kafka.send_messages(&topic, &messages).await;
            AsyncCommandOutput::SendResult
        });
    }
    fn get_key(
        &mut self,
        widgets: &mut MessagesSendDialogModelWidgets,
        is_multi: bool,
    ) -> Vec<String> {
        info!("get_key");
        let (start, end) = widgets.single_message_key.buffer().bounds();
        let key = widgets
            .single_message_key
            .buffer()
            .text(&start, &end, true)
            .to_string();
        let key = if !key.trim().is_empty() {
            Some(key)
        } else {
            None
        };
        if is_multi {
            key.map_or(vec![], |text| {
                text.lines().map(|s| s.to_string()).into_iter().collect()
            })
        } else {
            key.map_or(vec![], |text| vec![text])
        }
    }
    fn get_value(
        &mut self,
        widgets: &mut MessagesSendDialogModelWidgets,
        is_multi: bool,
    ) -> Vec<String> {
        info!("get_value");
        let (start, end) = widgets.single_message_value.buffer().bounds();
        let key = widgets
            .single_message_value
            .buffer()
            .text(&start, &end, true)
            .to_string();
        let key = if !key.trim().is_empty() {
            Some(key)
        } else {
            None
        };
        if is_multi {
            key.map_or(vec![], |text| {
                text.lines().map(|s| s.to_string()).into_iter().collect()
            })
        } else {
            key.map_or(vec![], |text| vec![text])
        }
    }
    fn get_key_value(
        &mut self,
        widgets: &mut MessagesSendDialogModelWidgets,
        is_multi: bool,
    ) -> Vec<(String, String)> {
        info!("get_key_value");
        let separator = widgets.multiple_key_value_separator.text().to_string();
        let separator = if separator.trim().is_empty() {
            ","
        } else {
            separator.as_str()
        };
        let (start, end) = widgets.single_message_value.buffer().bounds();
        let key = widgets
            .single_message_value
            .buffer()
            .text(&start, &end, true)
            .to_string();
        debug!("get_key_value::value::{}", &key);
        let key = if !key.trim().is_empty() {
            Some(key)
        } else {
            None
        };
        if is_multi {
            key.map_or(vec![], |text| {
                text.lines()
                    .map(|s| {
                        trace!("get_key_value::line::[separator={}]:{}", separator, s);
                        let tokenized: Vec<&str> = s.splitn(2, separator).collect();
                        trace!(
                            "get_key_value::tokenized::[{}]::{:?}",
                            tokenized.len(),
                            tokenized
                        );
                        if tokenized.len() == 2 {
                            (
                                tokenized.first().unwrap().to_string(),
                                tokenized.last().unwrap().to_string(),
                            )
                        } else {
                            ("".to_string(), "".to_string())
                        }
                    })
                    .into_iter()
                    .collect()
            })
        } else {
            vec![]
        }
    }
    fn send_single_message(
        &mut self,
        widgets: &mut MessagesSendDialogModelWidgets,
        sender: ComponentSender<Self>,
    ) {
        let partition = self.selected_partition.unwrap_or(0);
        let topic = self.topic.clone().unwrap().name;
        let key = self.get_key(widgets, false);
        let value = self.get_value(widgets, false);
        if !value.is_empty() {
            let message = KrustMessage {
                topic: topic.clone(),
                partition,
                offset: 0,
                key: key.first().cloned(),
                value: value.first().unwrap().to_string(),
                timestamp: None,
                headers: vec![],
            };
            let connection = self.connection.clone().unwrap();
            let messages = vec![message];
            sender.oneshot_command(async move {
                // Run async background task
                let kafka = KafkaBackend::new(&connection);
                kafka.send_messages(&topic, &messages).await;
                AsyncCommandOutput::SendResult
            });
        }
    }
}
