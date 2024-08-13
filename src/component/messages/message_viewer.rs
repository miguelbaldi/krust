use gtk::prelude::*;
use relm4::{typed_view::column::TypedColumnView, *};
use sourceview::prelude::*;
use sourceview5 as sourceview;

use crate::{
    backend::repository::KrustHeader,
    component::messages::lists::{HeaderNameColumn, HeaderValueColumn},
};

use super::lists::HeaderListItem;

#[derive(Debug)]
pub struct MessageViewerInit {}
#[derive(Debug)]
pub enum MessageViewerMsg {
    Open(String, Vec<KrustHeader>),
    Clear,
}

#[derive(Debug)]
pub struct MessageViewerModel {
    headers_wrapper: TypedColumnView<HeaderListItem, gtk::NoSelection>,
}

#[relm4::component(pub)]
impl Component for MessageViewerModel {
    type Init = ();
    type Input = MessageViewerMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Stack {
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
        }
    }
    fn init(
        _: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut headers_wrapper = TypedColumnView::<HeaderListItem, gtk::NoSelection>::new();
        headers_wrapper.append_column::<HeaderNameColumn>();
        headers_wrapper.append_column::<HeaderValueColumn>();
        let headers_view = headers_wrapper.view.clone();
        let model = MessageViewerModel { headers_wrapper };
        let widgets = view_output!();

        let buffer = widgets
            .value_source_view
            .buffer()
            .downcast::<sourceview::Buffer>()
            .expect("sourceview was not backed by sourceview buffer");
        buffer.set_highlight_syntax(true);
        if let Some(scheme) = &sourceview::StyleSchemeManager::new().scheme("oblivion") {
            buffer.set_style_scheme(Some(scheme));
        }

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match msg {
            MessageViewerMsg::Open(message_text, headers) => {
                let buffer = widgets
                    .value_source_view
                    .buffer()
                    .downcast::<sourceview::Buffer>()
                    .expect("sourceview was not backed by sourceview buffer");

                let valid_json: Result<serde_json::Value, _> =
                    serde_json::from_str(message_text.as_str());
                let (language, formatted_text) = match valid_json {
                    Ok(jt) => (
                        sourceview::LanguageManager::default().language("json"),
                        serde_json::to_string_pretty(&jt).unwrap(),
                    ),
                    Err(_) => (
                        sourceview::LanguageManager::default().language("text"),
                        message_text,
                    ),
                };

                buffer.set_language(language.as_ref());
                buffer.set_text(&formatted_text.clone());
                widgets.value_source_view.queue_allocate();

                self.headers_wrapper.clear();
                for header in headers.iter() {
                    self.headers_wrapper
                        .append(HeaderListItem::new(header.clone()));
                }
            }
            MessageViewerMsg::Clear => {
                widgets.value_source_view.buffer().set_text("");
                widgets.value_source_view.queue_allocate();
                self.headers_wrapper.clear();
            }
        };

        self.update_view(widgets, sender);
    }
}
