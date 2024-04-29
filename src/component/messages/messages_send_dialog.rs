#![allow(deprecated)]
use adw::prelude::*;
use relm4::*;
use relm4_components::simple_adw_combo_row::{SimpleComboRow, SimpleComboRowMsg};
use tracing::*;

use crate::backend::{
    kafka::KafkaBackend,
    repository::{KrustConnection, KrustTopic},
};

#[derive(Debug)]
pub struct MessagesSendDialogModel {
    pub connection: Option<KrustConnection>,
    pub topic: Option<KrustTopic>,
    pub partitions_combo_row: Controller<SimpleComboRow<String>>,
    pub selected_partition: Option<i32>,
}

#[derive(Debug)]
pub enum MessagesSendDialogMsg {
    PartitionSelected(usize),
    LoadPartitions,
}

#[derive(Debug)]
pub enum AsyncCommandOutput {
    SetPartitions(Vec<String>),
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
            set_content_height: 768,
            set_content_width: 1024,
            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                adw::HeaderBar {},
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_valign: gtk::Align::Start,
                    set_margin_all: 10,
                    adw::PreferencesGroup {
                        set_title: "Settings",
                        #[local_ref]
                        partitions_combo_row -> adw::ComboRow {
                            set_title: "Partition",
                            set_subtitle: "Select target topic partition",
                        },
                        #[name(toggle_multiple_messages)]
                        adw::SwitchRow {
                            set_title: "Multiple",
                            set_subtitle: "Enable multiple messages mode",
                        },
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
                                    set_height_request: 200,
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
                                set_height_request: 400,
                                #[name(single_message_value)]
                                gtk::TextView {
                                    set_vexpand: true,
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
                        },
                        #[name(single_message_cancel)]
                        gtk::Button {
                            set_label: "Cancel",
                            set_margin_start: 10,
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
        let partitions_combo_row = SimpleComboRow::builder()
        .launch(SimpleComboRow {
            variants: vec!["0".to_string(), "1".to_string()],
            active_index: Some(default_idx),
        })
        .forward(
            sender.input_sender(),
            MessagesSendDialogMsg::PartitionSelected,
        );

        let model = MessagesSendDialogModel {
            connection: connection,
            topic: topic,
            partitions_combo_row: partitions_combo_row,
            selected_partition: None,
        };
        let partitions_combo_row = model.partitions_combo_row.widget();
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
        _: &Self::Root,
    ) {
        info!("received message: {:?}", msg);

        match msg {
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
                let partition_id = match self.partitions_combo_row.model().get_active_elem() {
                    Some(opt) => opt.clone().parse::<i32>().unwrap_or_default(),
                    None => 0,
                };
                info!("selected partition {}", partition_id);
                self.selected_partition = Some(partition_id);
            }
        };

        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        _widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match message {
            AsyncCommandOutput::SetPartitions(partitions) => {
                let variants = partitions.clone();
                self.partitions_combo_row
                .emit(SimpleComboRowMsg::UpdateData(SimpleComboRow {
                    variants: variants,
                    active_index: Some(0),
                }));
            }
        }
    }
}
