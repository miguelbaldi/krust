#![allow(deprecated)]
use adw::prelude::*;
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
    PartitionSelected(usize),
    LoadPartitions,
    ToggleMultipleMessages(bool),
    MultiFormatSelected(usize),
    Cancel,
    Send,
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
        adw::Dialog {
            set_title: "Add messages",
            set_content_height: 650,
            set_content_width: 900,
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
                        set_vexpand: true,
                        set_hexpand: true,
                        adw::ActionRow {
                            set_title: "Key",
                            set_subtitle: "Key text goes here",
                            //set_activatable_widget: Some(&single_message_text),
                            #[wrap(Some)]
                            set_child: single_message_key_container = &gtk::ScrolledWindow {
                                set_vexpand: true,
                                set_hexpand: true,
                                set_propagate_natural_height: true,
                                set_overflow: gtk::Overflow::Hidden,
                                set_valign: gtk::Align::Fill,
                                #[name(single_message_key)]
                                gtk::TextView {
                                    set_top_margin: 5,
                                    set_left_margin: 5,
                                    set_height_request: 200,
                                    set_monospace: true,
                                    add_css_class: "message-textview",
                                },
                            },
                        },
                    },
                    #[name(single_message_value_group)]
                    adw::PreferencesGroup {
                        set_title: "Message",
                        set_margin_top: 10,
                        set_vexpand: true,
                        set_hexpand: true,
                        set_valign: gtk::Align::Fill,
                        add_css_class: "message-group",
                        adw::ActionRow {
                            set_title: "Message",
                            set_subtitle: "Message text goes here",
                            set_vexpand: true,
                            //set_activatable_widget: Some(&single_message_text),
                            #[wrap(Some)]
                            set_child: single_message_value_container = &gtk::ScrolledWindow {
                                set_vexpand: true,
                                set_hexpand: true,
                                set_propagate_natural_height: true,
                                set_overflow: gtk::Overflow::Hidden,
                                set_valign: gtk::Align::Fill,
                                //set_height_request: 400,
                                set_min_content_height: 200,
                                #[name(single_message_value)]
                                gtk::TextView {
                                    set_vexpand: true,
                                    set_monospace: true,
                                    set_top_margin: 5,
                                    set_left_margin: 5,
                                    add_css_class: "message-textview",
                                },
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
        //let security_type_combo = model.security_type_combo.widget();
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
            MessagesSendDialogMsg::Cancel => {
                root.close();
            }
            MessagesSendDialogMsg::Send => {
                if self.is_multiple {
                    match self.selected_multi_format {
                        Some(MultiFormat::Key) => {}
                        Some(MultiFormat::Value) => {}
                        Some(MultiFormat::KeyValue) => {}
                        None => {}
                    }
                    let alert =adw::AlertDialog::builder()
                    .heading("Error")
                    .title("Error")
                        .body("Sorry, no donuts for you\nUnder construction!")
                        .close_response("close")
                        .default_response("close")
                        .can_close(true)
                        .receives_default(true)
                        .build();
                    alert.add_response("close", "Cancel");
                    alert.present(root);
                } else {
                    let partition = self.selected_partition.unwrap_or(0);
                    let (start, end) = widgets.single_message_key.buffer().bounds();
                    let key = widgets
                        .single_message_key
                        .buffer()
                        .text(&start, &end, true)
                        .to_string();
                    let key = if !key.is_empty() { Some(key) } else { None };
                    let (start, end) = widgets.single_message_value.buffer().bounds();
                    let value = widgets
                        .single_message_value
                        .buffer()
                        .text(&start, &end, true)
                        .to_string();
                    let topic = self.topic.clone().unwrap().name;
                    let message = KrustMessage {
                        topic: topic.clone(),
                        partition,
                        offset: 0,
                        key,
                        value,
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
