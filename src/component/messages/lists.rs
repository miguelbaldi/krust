// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use crate::backend::repository::{KrustHeader, KrustMessage};
use chrono::prelude::*;
use chrono_tz::America;
use gtk::prelude::*;
use relm4::{
    typed_view::{
        column::{LabelColumn, RelmColumn},
        OrdFn,
    },
    *,
};

// Table headers: start
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderListItem {
    pub name: String,
    pub value: Option<String>,
}

impl HeaderListItem {
    pub fn new(value: KrustHeader) -> Self {
        Self {
            name: value.key,
            value: value.value,
        }
    }
}

pub struct HeaderNameColumn;

impl LabelColumn for HeaderNameColumn {
    type Item = HeaderListItem;
    type Value = String;

    const COLUMN_NAME: &'static str = "Name";

    const ENABLE_SORT: bool = true;
    const ENABLE_RESIZE: bool = true;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.name.clone()
    }

    fn format_cell_value(value: &Self::Value) -> String {
        value.to_string()
    }
}
pub struct HeaderValueColumn;

impl HeaderValueColumn {
    fn format_cell_value(value: &String) -> String {
        value.to_string()
    }
    fn get_cell_value(item: &HeaderListItem) -> String {
        item.value.clone().unwrap_or_default()
    }
}

impl RelmColumn for HeaderValueColumn {
    type Root = gtk::Label;
    type Widgets = ();
    type Item = HeaderListItem;

    const COLUMN_NAME: &'static str = "Value";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        (gtk::Label::new(None), ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, label: &mut Self::Root) {
        label.set_label(&HeaderValueColumn::format_cell_value(
            &HeaderValueColumn::get_cell_value(item),
        ));
        label.set_halign(gtk::Align::Start);
    }

    fn sort_fn() -> OrdFn<Self::Item> {
        Some(Box::new(|a: &HeaderListItem, b: &HeaderListItem| {
            a.value.cmp(&b.value)
        }))
    }
}
// Table headers: end

// Table messages: start
#[derive(Debug)]
pub struct MessageListItem {
    pub offset: i64,
    pub partition: i32,
    pub key: String,
    pub value: String,
    pub timestamp: Option<i64>,
    pub headers: Vec<KrustHeader>,
    pub timestamp_formatter: String,
}

impl PartialEq for MessageListItem {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset
            && self.key == other.key
            && self.value == other.value
            && self.timestamp == other.timestamp
    }
}
impl Eq for MessageListItem {}

impl MessageListItem {
    pub fn new(value: KrustMessage, timestamp_formatter: String) -> Self {
        Self {
            offset: value.offset,
            partition: value.partition,
            key: value.key.unwrap_or_default(),
            value: value.value,
            timestamp: value.timestamp,
            headers: value.headers,
            timestamp_formatter,
        }
    }
}

pub struct MessageOffsetColumn;

impl LabelColumn for MessageOffsetColumn {
    type Item = MessageListItem;
    type Value = i64;

    const COLUMN_NAME: &'static str = "Offset";

    const ENABLE_SORT: bool = true;
    const ENABLE_RESIZE: bool = true;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.offset
    }

    fn format_cell_value(value: &Self::Value) -> String {
        format!("{}", value)
    }
}

pub struct MessagePartitionColumn;

impl LabelColumn for MessagePartitionColumn {
    type Item = MessageListItem;
    type Value = i32;

    const COLUMN_NAME: &'static str = "Partition";

    const ENABLE_SORT: bool = true;
    const ENABLE_RESIZE: bool = true;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.partition
    }

    fn format_cell_value(value: &Self::Value) -> String {
        format!("{}", value)
    }
}

pub struct MessageTimestampColumn;

impl RelmColumn for MessageTimestampColumn {
    type Root = gtk::Label;
    type Widgets = ();
    type Item = MessageListItem;

    const COLUMN_NAME: &'static str = "Date/time (Timestamp)";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = false;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Center);
        (label, ())
    }

    fn bind(item: &mut Self::Item, _: &mut Self::Widgets, label: &mut Self::Root) {
        let formatted = format!(
            "{}",
            Utc.timestamp_millis_opt(item.timestamp.unwrap_or_default())
                .unwrap()
                .with_timezone(&America::Sao_Paulo)
                .format(&item.timestamp_formatter)
        );
        label.set_label(&formatted);
    }

    fn sort_fn() -> OrdFn<Self::Item> {
        Some(Box::new(|a: &MessageListItem, b: &MessageListItem| {
            a.timestamp
                .unwrap_or_default()
                .cmp(&b.timestamp.unwrap_or_default())
        }))
    }
}

pub struct MessageValueColumn;

impl RelmColumn for MessageValueColumn {
    type Root = gtk::Label;
    type Widgets = ();
    type Item = MessageListItem;

    const COLUMN_NAME: &'static str = "Value";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        (label, ())
    }

    fn bind(item: &mut Self::Item, _widgets: &mut Self::Widgets, label: &mut Self::Root) {
        let formatted = item.value.replace('\n', " ").clone();
        label.set_label(&formatted);
    }
}
pub struct MessageKeyColumn;

impl LabelColumn for MessageKeyColumn {
    type Item = MessageListItem;
    type Value = String;

    const COLUMN_NAME: &'static str = "Key";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;
    const ENABLE_SORT: bool = false;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.key.clone()
    }

    fn format_cell_value(value: &Self::Value) -> String {
        if value.len() >= 40 {
            format!("{}...", value.replace('\n', " ").get(0..40).unwrap_or(""))
        } else {
            value.to_string()
        }
    }
}

// Table messages: end
