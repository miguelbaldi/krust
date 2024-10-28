// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use adw::prelude::*;
use fs_extra::dir::get_size;
use gtk::{glib::SignalHandlerId, ColumnViewColumn};
use humansize::{format_size, DECIMAL};
use std::{cell::RefCell, cmp::Ordering, fs, path::Path};
use sysinfo::Disks;

use relm4::{
    gtk,
    typed_view::{
        column::{RelmColumn, TypedColumnView},
        OrdFn,
    },
    view, Component, ComponentParts, ComponentSender, RelmWidgetExt,
};

use tracing::*;

use crate::{
    backend::{
        repository::MessagesRepository,
        settings::Settings,
        worker::{MessagesCleanupRequest, MessagesWorker},
    },
    Repository,
};

// Table: start
#[derive(Debug)]
pub struct TopicListItem {
    topic_name: String,
    connection_name: String,
    connection_id: usize,
    cache_folder_size: usize,
    topic_size: usize,
    sender: ComponentSender<CacheManagerDialogModel>,
    clicked_handler_id: RefCell<Option<SignalHandlerId>>,
}

impl TopicListItem {
    fn new(
        topic_name: String,
        conn_name: String,
        conn_id: usize,
        cache_folder_size: usize,
        topic_size: usize,
        sender: ComponentSender<CacheManagerDialogModel>,
    ) -> Self {
        Self {
            topic_name,
            connection_name: conn_name,
            connection_id: conn_id,
            cache_folder_size,
            topic_size,
            sender,
            clicked_handler_id: RefCell::new(None),
        }
    }
}

impl Eq for TopicListItem {}

impl Ord for TopicListItem {
    fn cmp(&self, other: &Self) -> Ordering {
        //self.partial_cmp(other).unwrap()
        match PartialOrd::partial_cmp(&self.topic_size, &other.topic_size) {
            Some(Ordering::Equal) => {
                match PartialOrd::partial_cmp(&self.topic_name, &other.topic_name) {
                    Some(Ordering::Equal) => {
                        PartialOrd::partial_cmp(&self.connection_id, &other.connection_id).unwrap()
                    }
                    cmp => cmp.unwrap(),
                }
            }
            Some(Ordering::Less) => Ordering::Greater,
            Some(Ordering::Greater) => Ordering::Less,
            cmp => cmp.unwrap(),
        }
    }
}

impl PartialEq for TopicListItem {
    fn eq(&self, other: &Self) -> bool {
        self.topic_size == other.topic_size
            && self.topic_name == other.topic_name
            && self.connection_id == other.connection_id
    }
}

impl PartialOrd for TopicListItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct TopicColumn;

impl RelmColumn for TopicColumn {
    type Item = TopicListItem;
    type Root = gtk::Label;
    type Widgets = ();

    const COLUMN_NAME: &'static str = "Topic";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;
    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        (label, ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, label: &mut Self::Root) {
        label.set_label(&item.topic_name);
    }

    fn sort_fn() -> OrdFn<Self::Item> {
        Some(Box::new(|a: &TopicListItem, b: &TopicListItem| {
            a.topic_name.cmp(&b.topic_name)
        }))
    }
}

struct ConnectionColumn;

impl RelmColumn for ConnectionColumn {
    type Item = TopicListItem;
    type Root = gtk::Label;
    type Widgets = ();

    const COLUMN_NAME: &'static str = "Connection";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;
    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        (label, ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, label: &mut Self::Root) {
        label.set_label(&item.connection_name);
    }

    fn sort_fn() -> OrdFn<Self::Item> {
        Some(Box::new(|a: &TopicListItem, b: &TopicListItem| {
            a.connection_name.cmp(&b.connection_name)
        }))
    }
}

struct DiskUsageColumnWidgets {
    bar: gtk::LevelBar,
    bar_text: gtk::Label,
    button: gtk::Button,
}

struct DiskUsageColumn;

impl RelmColumn for DiskUsageColumn {
    type Root = gtk::Box;
    type Widgets = DiskUsageColumnWidgets;
    type Item = TopicListItem;

    const COLUMN_NAME: &'static str = "Disk usage";
    const ENABLE_RESIZE: bool = false;
    const ENABLE_EXPAND: bool = true;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        view! {
            root_box = gtk::Box {
                #[name(label)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 2,
                    set_hexpand: false,
                    set_vexpand: false,
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Center,
                    set_width_request: 200,
                    #[name(button)]
                    gtk::Button {
                        set_tooltip_text: Some("Delete selected topic"),
                        set_icon_name: "edit-delete-symbolic",
                        set_margin_start: 2,
                        add_css_class: "krust-destroy",
                    },
                    #[name(bar)]
                    gtk::LevelBar {
                        set_width_request: 150,
                        set_height_request: 10,
                        set_vexpand: false,
                        set_valign: gtk::Align::Center,
                    },
                    #[name(bar_text)]
                    gtk::Label {

                    },
                },
            }
        }
        (
            root_box,
            DiskUsageColumnWidgets {
                bar,
                bar_text,
                button,
            },
        )
    }

    fn bind(item: &mut Self::Item, widgets: &mut Self::Widgets, _box: &mut Self::Root) {
        let topic_name = item.topic_name.clone();
        let connection_id = item.connection_id;
        let sender = item.sender.clone();
        let signal_id = widgets.button.connect_clicked(move |_b| {
            info!("TopicColumn[{}]", &topic_name);
            sender.input(CacheManagerDialogMsg::DeleteTopicCache {
                topic_name: topic_name.clone(),
                connection_id,
            });
        });
        item.clicked_handler_id = RefCell::new(Some(signal_id));

        let fraction = item.topic_size as f64 / item.cache_folder_size as f64;
        let percentage = format!("{:.1}%", 100.0 * fraction);
        let text = format!(
            "{} / {} ({})",
            format_size(item.topic_size, DECIMAL),
            format_size(item.cache_folder_size, DECIMAL),
            percentage,
        );
        widgets.bar.add_offset_value(gtk::LEVEL_BAR_OFFSET_LOW, 0.5);
        widgets
            .bar
            .add_offset_value(gtk::LEVEL_BAR_OFFSET_HIGH, 0.90);

        widgets.bar.set_min_value(0.0);
        widgets.bar.set_max_value(1.0);
        widgets.bar.set_value(fraction);
        //widgets.bar.set_value(0.91);
        widgets.bar_text.set_label(text.as_str());
    }
    fn unbind(item: &mut Self::Item, widgets: &mut Self::Widgets, _box: &mut Self::Root) {
        if let Some(id) = item.clicked_handler_id.take() {
            widgets.button.disconnect(id);
        };
    }
    fn sort_fn() -> OrdFn<Self::Item> {
        Some(Box::new(|a: &TopicListItem, b: &TopicListItem| {
            a.topic_size.cmp(&b.topic_size)
        }))
    }
}
// Table: end

#[derive(Debug)]
pub struct CacheManagerDialogModel {
    cache_dir: String,
    pub topics_wrapper: TypedColumnView<TopicListItem, gtk::SingleSelection>,
}

#[derive(Debug)]
pub enum CacheManagerDialogMsg {
    Show,
    DeleteTopicCache {
        connection_id: usize,
        topic_name: String,
    },
    Refresh,
}

pub struct CacheManagerDialogInit {}

#[relm4::component(pub)]
impl Component for CacheManagerDialogModel {
    type CommandOutput = ();
    type Input = CacheManagerDialogMsg;
    type Output = ();
    type Init = CacheManagerDialogInit;

    view! {
        #[root]
        adw::Dialog {
            set_title: "Cache Manager",
            #[wrap(Some)]
            set_child = &gtk::Box {
                adw::HeaderBar {
                    pack_end = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        gtk::Button {
                            set_tooltip_text: Some("Refresh disk/cache usage"),
                            set_icon_name: "media-playlist-repeat-symbolic",
                            set_margin_end: 5,
                            add_css_class: "circular",
                            connect_clicked[sender] => move |_| {
                                sender.input(CacheManagerDialogMsg::Refresh);
                            },
                        }
                    },
                },
                set_valign: gtk::Align::Fill,
                set_orientation: gtk::Orientation::Vertical,
                gtk::Box {
                    set_valign: gtk::Align::Fill,
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_width_request: 900,
                    set_height_request: 600,
                    set_spacing: 10,
                    set_margin_all: 20,
                    adw::PreferencesGroup {
                        set_title: "Disk usage",
                        #[name(total_space_label)]
                        gtk::Label {},
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 5,
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            set_width_request: 580,
                            #[name(total_space_bar)]
                            gtk::LevelBar {
                                set_width_request: 400,
                            },
                            #[name(total_space_bar_text)]
                            gtk::Label {},
                        },
                        #[name(cache_space_label)]
                        gtk::Label {
                            set_margin_top: 10,
                        },
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 5,
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            set_width_request: 580,
                            #[name(cache_space_bar)]
                            gtk::LevelBar {
                                set_width_request: 400,
                            },
                            #[name(cache_space_bar_text)]
                            gtk::Label {},
                        },
                    },
                    adw::PreferencesGroup {
                        set_title: "Topics",
                        #[name(topics_scrolled_windows)]
                        gtk::ScrolledWindow {
                            set_vexpand: true,
                            set_hexpand: true,
                            set_propagate_natural_width: true,
                            set_vscrollbar_policy: gtk::PolicyType::Always,
                            model.topics_wrapper.view.clone() -> gtk::ColumnView {
                                set_vexpand: true,
                                set_hexpand: true,
                                set_show_row_separators: true,
                            }
                        }
                    }
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let settings = Settings::read().unwrap_or_default();
        // Initialize the ListView wrapper
        let mut view_wrapper = TypedColumnView::<TopicListItem, gtk::SingleSelection>::new();
        view_wrapper.append_column::<DiskUsageColumn>();
        view_wrapper.append_column::<ConnectionColumn>();
        view_wrapper.append_column::<TopicColumn>();
        let sort_column: Option<&ColumnViewColumn> =
            view_wrapper.get_columns().get(DiskUsageColumn::COLUMN_NAME);
        let sort_type = gtk::SortType::Descending;
        info!(
            "sort_column::{:?}, sort_type::{:?}",
            sort_column.map(|c| c.title()),
            sort_type
        );
        view_wrapper.view.sort_by_column(sort_column, sort_type);

        let model = CacheManagerDialogModel {
            cache_dir: settings.cache_dir,
            topics_wrapper: view_wrapper,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            CacheManagerDialogMsg::Show => {
                let parent = &relm4::main_application().active_window().unwrap();
                sender.input(CacheManagerDialogMsg::Refresh);
                root.queue_allocate();
                root.present(parent);
            }
            CacheManagerDialogMsg::Refresh => {
                self.topics_wrapper.clear();
                let mut disks = Disks::new_with_refreshed_list();
                let settings = Settings::read().unwrap_or_default();
                self.cache_dir = settings.cache_dir.clone();
                self.load_disk_usage_info(settings, widgets, &mut disks);
                let cache_dir_path = Path::new(&self.cache_dir);
                let cache_dir_size = get_size(cache_dir_path).unwrap_or(0) as usize;
                let paths = fs::read_dir(cache_dir_path).unwrap();
                for path in paths {
                    let file = path.unwrap();
                    let file_name = file.file_name();
                    let cache_size = get_size(file.path()).unwrap_or(0) as usize;
                    let cache_size_formatted =
                        format_size(get_size(file.path()).unwrap_or(0), DECIMAL);
                    let repo = MessagesRepository::from_filename(
                        file_name.to_str().unwrap_or_default().to_string(),
                    );
                    let conn = Repository::new().connection_by_id(repo.connection_id);
                    if let Some(conn) = conn {
                        let item = TopicListItem::new(
                            repo.topic_name,
                            conn.name,
                            conn.id.unwrap_or_default(),
                            cache_dir_size,
                            cache_size,
                            sender.clone(),
                        );
                        self.topics_wrapper.append(item);
                        info!(
                            "Name: {} - {}",
                            file_name.to_str().unwrap_or_default(),
                            cache_size_formatted,
                        );
                    }
                }
            }
            CacheManagerDialogMsg::DeleteTopicCache {
                connection_id,
                topic_name,
            } => {
                info!(
                    "delete topic cache [connection_id={}, topic={}]",
                    connection_id, topic_name
                );
                let idx = self
                    .topics_wrapper
                    .find(|i| i.connection_id == connection_id && i.topic_name == topic_name);
                if let Some(idx) = idx {
                    let worker = MessagesWorker::new();
                    let maybe_topic = worker.cleanup_messages(&MessagesCleanupRequest {
                        connection_id,
                        topic_name,
                        refresh: true,
                    });
                    if let Some(_topic) = maybe_topic {
                        self.topics_wrapper.remove(idx);
                    }
                }
            }
        }
    }
}

impl CacheManagerDialogModel {
    fn load_disk_usage_info(
        &mut self,
        settings: Settings,
        widgets: &mut CacheManagerDialogModelWidgets,
        disks: &mut Disks,
    ) {
        let cache_dir_path = Path::new(&settings.cache_dir);
        let reverse_disks = disks.list_mut();
        reverse_disks.sort_by(|a, b| b.mount_point().as_os_str().cmp(a.mount_point().as_os_str()));
        let disk = reverse_disks
            .iter()
            .find(|d| cache_dir_path.starts_with(d.mount_point()));
        if let Some(current_disk) = disk {
            let used_space = current_disk.total_space() - current_disk.available_space();
            let fraction = used_space as f64 / current_disk.total_space() as f64;
            let percentage = format!("{:.1}%", 100.0 * fraction);
            let text = format!(
                "{} / {} ({})",
                format_size(used_space, DECIMAL),
                format_size(current_disk.total_space(), DECIMAL),
                percentage,
            );
            let disk_name = current_disk.mount_point().to_str().unwrap_or("");
            widgets
                .total_space_label
                .set_label(format!("Disk [{}]", disk_name).as_str());
            widgets
                .total_space_bar
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_LOW, 0.5);
            widgets
                .total_space_bar
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_HIGH, 0.90);

            widgets.total_space_bar.set_min_value(0.0);
            widgets.total_space_bar.set_max_value(1.0);
            widgets.total_space_bar.set_value(fraction);
            //widgets.total_space_bar.set_value(0.91);
            widgets.total_space_bar_text.set_label(text.as_str());

            let cache_dir_size = get_size(settings.cache_dir.clone()).unwrap_or(0);
            let fraction = cache_dir_size as f64 / used_space as f64;
            let percentage = format!("{:.1}%", 100.0 * fraction);
            let text = format!(
                "{} / {} ({})",
                format_size(cache_dir_size, DECIMAL),
                format_size(used_space, DECIMAL),
                percentage,
            );
            widgets.cache_space_label.set_label(
                format!("Cache folder [{}]", cache_dir_path.to_str().unwrap_or("")).as_str(),
            );
            widgets
                .cache_space_bar
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_LOW, 0.5);
            widgets
                .cache_space_bar
                .add_offset_value(gtk::LEVEL_BAR_OFFSET_HIGH, 0.90);
            widgets.cache_space_bar.set_min_value(0.0);
            widgets.cache_space_bar.set_max_value(1.0);
            widgets.cache_space_bar.set_value(fraction);
            widgets.cache_space_bar_text.set_label(text.as_str());
        }
    }
}
