// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use crate::backend::{
    kafka::{CreateTopicRequest, KafkaBackend},
    repository::KrustConnection,
};
use adw::prelude::*;
use gtk::Adjustment;
use relm4::*;

use tracing::*;

#[derive(Debug)]
pub struct CreateTopicDialogModel {
    pub connection: Option<KrustConnection>,
    pub partition_count: Option<u16>,
    pub replica_count: Option<u8>,
}

#[derive(Debug)]
pub enum CreateTopicDialogMsg {
    Show,
    Create,
    Cancel,
    Close,
    SetPartitionCount,
    SetReplicaCount,
}

#[derive(Debug)]
pub enum CreateTopicDialogOutput {
    RefreshTopics,
}

#[derive(Debug)]
pub enum AsyncCommandOutput {
    CreateResult,
}

#[relm4::component(pub)]
impl Component for CreateTopicDialogModel {
    type Init = Option<KrustConnection>;
    type Input = CreateTopicDialogMsg;
    type Output = CreateTopicDialogOutput;
    type CommandOutput = AsyncCommandOutput;

    view! {
        #[root]
        main_dialog = adw::Dialog {
            set_title: "Create topic",
            set_content_width: 400,
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
                    #[name(create_topic_settings_group)]
                    adw::PreferencesGroup {
                        set_title: "Settings",
                        set_margin_top: 10,
                        set_vexpand: false,
                        set_hexpand: true,
                        #[name(name)]
                        adw::EntryRow {
                            set_title: "Name",
                        },
                        #[name(partition_count)]
                        adw::SpinRow {
                            set_title: "Partition count",
                            set_subtitle: "Number of partitions",
                            set_snap_to_ticks: true,
                            set_numeric: true,
                            set_wrap: false,
                            set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                            connect_value_notify => CreateTopicDialogMsg::SetPartitionCount,
                        },
                        #[name(replica_count)]
                        adw::SpinRow {
                            set_title: "Replica count",
                            set_subtitle: "Number of replicas",
                            set_snap_to_ticks: true,
                            set_numeric: true,
                            set_wrap: false,
                            set_update_policy: gtk::SpinButtonUpdatePolicy::IfValid,
                            connect_value_notify => CreateTopicDialogMsg::SetReplicaCount,
                        },
                    },
                    gtk::Box {
                        set_margin_top: 10,
                        set_margin_bottom: 10,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::End,
                        #[name(single_message_send)]
                        gtk::Button {
                            set_label: "Create",
                            add_css_class: "destructive-action",
                            connect_clicked[sender] => move |_| {
                                sender.input(CreateTopicDialogMsg::Create);
                            },
                        },
                        #[name(single_message_cancel)]
                        gtk::Button {
                            set_label: "Cancel",
                            set_margin_start: 10,
                            connect_clicked[sender] => move |_| {
                                sender.input(CreateTopicDialogMsg::Cancel);
                            },
                        },
                    }
                },
            },
            connect_closed[sender] => move |_this| {
                sender.input(CreateTopicDialogMsg::Close);
            },
        }
    }

    fn init(
        current_connection: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let connection = current_connection.clone();

        let model = CreateTopicDialogModel {
            connection,
            partition_count: None,
            replica_count: None,
        };

        let widgets = view_output!();
        let adjustment_partition_count = Adjustment::builder()
            .lower(1.0)
            .upper(1000.0)
            .page_size(0.0)
            .step_increment(1.0)
            .value(1.0)
            .build();
        widgets
            .partition_count
            .set_adjustment(Some(&adjustment_partition_count));
        let adjustment_replica_count = Adjustment::builder()
            .lower(1.0)
            .upper(50.0)
            .page_size(0.0)
            .step_increment(1.0)
            .value(1.0)
            .build();
        widgets
            .replica_count
            .set_adjustment(Some(&adjustment_replica_count));
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: CreateTopicDialogMsg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        debug!("received message: {:?}", msg);

        match msg {
            CreateTopicDialogMsg::Show => {
                let parent = &relm4::main_application().active_window().unwrap();
                root.queue_allocate();
                root.present(parent);
            }
            CreateTopicDialogMsg::Cancel => {
                root.close();
            }
            CreateTopicDialogMsg::Create => {
                info!("create");
                let name: String = widgets.name.text().into();
                let partition_count = self.partition_count.unwrap_or(1);
                let replica_count = self.replica_count.unwrap_or(1);
                let connection = self.connection.clone().unwrap();
                sender.oneshot_command(async move {
                    let kafka = KafkaBackend::new(&connection);
                    let result = kafka
                        .create_topic(&CreateTopicRequest {
                            name: name.clone(),
                            partition_count,
                            replica_count,
                        })
                        .await;
                    match result {
                        Err(e) => {
                            error!("problem creating topic {}: {}", &name, e)
                        }
                        Ok(_) => {
                            info!("topic {} created", &name)
                        }
                    };
                    AsyncCommandOutput::CreateResult
                })
            }
            CreateTopicDialogMsg::Close => {
                info!("close");
            }
            CreateTopicDialogMsg::SetPartitionCount => {
                let value = widgets.partition_count.value();
                self.partition_count = Some(value as u16);
            }
            CreateTopicDialogMsg::SetReplicaCount => {
                let value = widgets.replica_count.value();
                self.replica_count = Some(value as u8);
            }
        };

        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AsyncCommandOutput::CreateResult => {
                info!("CreateResult");
                widgets.name.set_text("");
                sender
                    .output(CreateTopicDialogOutput::RefreshTopics)
                    .expect("should send refresh to output");
                root.close();
            }
        }
    }
}
