use gtk::prelude::*;
use relm4::{
  typed_view::column::TypedColumnView,
  *,
};
use sourceview::prelude::*;
use sourceview5 as sourceview;
use tracing::info;

use crate::{backend::repository::{KrustConnection, KrustMessage}, component::{messages::lists::{HeaderListItem, HeaderNameColumn, HeaderValueColumn, MessageListItem, MessageOfssetColumn, MessagePartitionColumn, MessageTimestampColumn, MessageValueColumn}, status_bar::{StatusBarMsg, STATUS_BROKER}}};

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
        set_orientation: gtk::Orientation::Vertical,
        set_hexpand: true,
        set_vexpand: true,
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
            gtk::Button { set_icon_name: "media-playback-start-symbolic", },
            gtk::Button { set_icon_name: "media-playback-stop-symbolic", set_margin_start: 5, },
          },
          #[wrap(Some)]
          set_end_widget = &gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_halign: gtk::Align::Fill,
            set_hexpand: true,
            #[name(topics_search_entry)]
            gtk::SearchEntry {
              set_hexpand: true,
              set_halign: gtk::Align::Fill,
              
            },
            gtk::Button {
              set_icon_name: "edit-find-symbolic",
              set_margin_start: 5,
            },
          },
        },
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
    messages_wrapper.append_column::<MessagePartitionColumn>();
    messages_wrapper.append_column::<MessageOfssetColumn>();
    messages_wrapper.append_column::<MessageValueColumn>();
    messages_wrapper.append_column::<MessageTimestampColumn>();
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
        
        let valid_json: Result<serde_json::Value, _> = serde_json::from_str(message_text.as_str());
        let (language, formatted_text) = match valid_json {
          Ok(jt) => {
            (sourceview::LanguageManager::default().language("json"), serde_json::to_string_pretty(&jt).unwrap())
          },
          Err(_) => (sourceview::LanguageManager::default().language("text"), message_text),
        };
        buffer.set_language(language.as_ref());
        buffer
        .set_text(formatted_text.as_str());
        
        self.headers_wrapper.clear();
        for header in item.borrow().headers.iter() {
          self.headers_wrapper.append(HeaderListItem::new(header.clone()));
        }
      }
    };
    
    self.update_view(widgets, sender);
  }
}
