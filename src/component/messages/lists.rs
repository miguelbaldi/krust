use crate::{
    backend::repository::{KrustHeader, KrustMessage},
    DATE_TIME_FORMAT,
};
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
    pub fn new(value: KrustMessage) -> Self {
        Self {
            offset: value.offset,
            partition: value.partition,
            key: "".to_string(),
            value: value.value,
            timestamp: value.timestamp,
            headers: value.headers,
        }
    }
}

pub struct MessageOfssetColumn;

impl LabelColumn for MessageOfssetColumn {
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

impl LabelColumn for MessageTimestampColumn {
    type Item = MessageListItem;
    type Value = i64;

    const COLUMN_NAME: &'static str = "Date/time (Timestamp)";

    const ENABLE_SORT: bool = true;
    const ENABLE_RESIZE: bool = true;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.timestamp.unwrap_or(Utc::now().timestamp())
    }

    fn format_cell_value(value: &Self::Value) -> String {
        format!(
            "{}",
            Utc.timestamp_millis_opt(*value)
                .unwrap()
                .with_timezone(&America::Sao_Paulo)
                .format(DATE_TIME_FORMAT)
        )
    }
}

pub struct MessageValueColumn;

impl LabelColumn for MessageValueColumn {
    type Item = MessageListItem;
    type Value = String;

    const COLUMN_NAME: &'static str = "Value";
    const ENABLE_RESIZE: bool = true;
    const ENABLE_EXPAND: bool = true;
    const ENABLE_SORT: bool = true;

    fn get_cell_value(item: &Self::Item) -> Self::Value {
        item.value.clone()
    }

    fn format_cell_value(value: &Self::Value) -> String {
        if value.len() >= 150 {
            format!(
                "{}...",
                value
                    .replace('\n', " ")
                    .get(0..150)
                    .unwrap_or("")
            )
        } else {
            format!("{}...", value)
        }
    }
}

// Table messages: end
