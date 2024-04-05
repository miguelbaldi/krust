use gtk::prelude::*;
use relm4::{
  typed_view::{column::{LabelColumn, RelmColumn, TypedColumnView}, OrdFn},
  *,
};
use sourceview::prelude::*;
use sourceview5 as sourceview;
use tracing::info;

use crate::{backend::repository::{KrustConnection, KrustHeader, KrustMessage}, component::status_bar::{StatusBarMsg, STATUS_BROKER}};

// Table headers: start
#[derive(Debug, PartialEq, Eq)]
struct HeaderListItem {
  name: String,
  value: Option<String>,
}

impl HeaderListItem {
  fn new(value: KrustHeader) -> Self {
    Self {
      name: value.key,
      value: value.value,
    }
  }
}

struct HeaderNameColumn;

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
    format!("{}", value)
  }
}
struct HeaderValueColumn;

impl HeaderValueColumn {
  fn format_cell_value(value: &String) -> String {
    format!("{}", value)
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
    label.set_label(&HeaderValueColumn::format_cell_value(&HeaderValueColumn::get_cell_value(item)));
    label.set_halign(gtk::Align::Start);
  }
  
  fn sort_fn() -> OrdFn<Self::Item> {
    Some(Box::new(|a: &HeaderListItem, b: &HeaderListItem| a.value.cmp(&b.value)))
  }
}
// Table headers: end

// Table messages: start
#[derive(Debug)]
struct MessageListItem {
  offset: i64,
  key: String,
  value: String,
  timestamp: Option<i64>,
  headers: Vec<KrustHeader>,
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
  fn new(value: KrustMessage) -> Self {
    Self {
      offset: value.offset,
      key: "".to_string(),
      value: value.value,
      timestamp: value.timestamp,
      headers: value.headers,
    }
  }
}

struct OfssetColumn;

impl LabelColumn for OfssetColumn {
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

struct ValueColumn;

impl LabelColumn for ValueColumn {
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
    if value.len() >= 200 {
      format!("{}...", value.replace("\n", " ").get(0..200).unwrap_or("").to_string())
    } else {
      format!("{}...", value)
    }
  }
}

// Table messages: end

#[derive(Debug)]
pub struct MessagesPageModel {
  pub current: Option<KrustConnection>,
  messages_wrapper: TypedColumnView<MessageListItem, gtk::SingleSelection>,
  headers_wrapper: TypedColumnView<HeaderListItem, gtk::NoSelection>,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
  List(Vec<KrustMessage>),
  Open(u32),
}

#[derive(Debug)]
pub enum MessagesPageOutput {
  _ShowMessages,
}

#[relm4::component(pub)]
impl Component for MessagesPageModel {
  type CommandOutput = ();
  
  type Init = Option<KrustConnection>;
  type Input = MessagesPageMsg;
  type Output = ();
  
  view! {
    #[root]
    gtk::Paned {
      set_orientation: gtk::Orientation::Vertical,
      //set_resize_start_child: true,
      #[wrap(Some)]
      set_start_child = &gtk::Box {
        set_hexpand: true,
        set_vexpand: true,
        gtk::ScrolledWindow {
          set_vexpand: true,
          set_hexpand: true,
          set_propagate_natural_width: true,
          #[local_ref]
          messages_view -> gtk::ColumnView {
            set_vexpand: true,
            set_hexpand: true,
            set_show_row_separators: true,
            set_show_column_separators: true,
            set_single_click_activate: true,
          }
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
              #[local_ref]
              headers_view -> gtk::ColumnView {
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
      },
    }
    
  }
  
  fn init(
    current: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    // Initialize the messages ListView wrapper
    let mut messages_wrapper = TypedColumnView::<MessageListItem, gtk::SingleSelection>::new();
    messages_wrapper.append_column::<OfssetColumn>();
    messages_wrapper.append_column::<ValueColumn>();
    // Initialize the headers ListView wrapper
    let mut headers_wrapper = TypedColumnView::<HeaderListItem, gtk::NoSelection>::new();
    headers_wrapper.append_column::<HeaderNameColumn>();
    headers_wrapper.append_column::<HeaderValueColumn>();
    
    let model = MessagesPageModel {
      current: current,
      messages_wrapper: messages_wrapper,
      headers_wrapper: headers_wrapper,
    };
    
    let messages_view = &model.messages_wrapper.view;
    let headers_view = &model.headers_wrapper.view;
    messages_view.connect_activate(move |_view, idx| {
      sender.input(MessagesPageMsg::Open(idx));
    });
    
    let widgets = view_output!();
    
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
    
    ComponentParts { model, widgets }
  }
  
  fn update_with_view(
    &mut self,
    widgets: &mut Self::Widgets,
    msg: MessagesPageMsg,
    sender: ComponentSender<Self>,
    _: &Self::Root,
  ) {
    info!("received message: {:?}", msg);
    
    match msg {
      MessagesPageMsg::List(messages) => {
        self.messages_wrapper.clear();
        for message in messages {
          self.messages_wrapper.append(MessageListItem::new(message));
        }
        widgets
        .value_source_view
        .buffer()
        .set_text("");
        STATUS_BROKER.send(StatusBarMsg::StopWithInfo { text: Some("Messages loaded!".into()) });
      }
      MessagesPageMsg::Open(message_idx) => {
        let item = self.messages_wrapper.get_visible(message_idx).unwrap();
        let message_text = item.borrow().value.clone();
        
        let buffer = widgets
        .value_source_view
        .buffer()
        .downcast::<sourceview::Buffer>()
        .expect("sourceview was not backed by sourceview buffer");
        
        let valid_json: Result<serde::de::IgnoredAny, _> = serde_json::from_str(message_text.as_str());
        let language = match valid_json {
          Ok(_) => sourceview::LanguageManager::default().language("json"),
          Err(_) => sourceview::LanguageManager::default().language("text"),
        };
        buffer.set_language(language.as_ref());
        buffer
        .set_text(message_text.as_str());
        
        self.headers_wrapper.clear();
        for header in item.borrow().headers.iter() {
          self.headers_wrapper.append(HeaderListItem::new(header.clone()));
        }
      }
    };
    
    self.update_view(widgets, sender);
  }
}
