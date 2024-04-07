//! Alert dialog for displaying arbitrary errors.
//!
//! Inspired by [`relm4_components::alert`], but allows sending the dialog text as part of the
//! `Show` message, and supports displaying only a single button to dismiss.

use std::time::Instant;

use gtk::prelude::*;
use relm4::prelude::*;
use relm4::MessageBroker;

pub static STATUS_BROKER: MessageBroker<StatusBarMsg> = MessageBroker::new();

#[derive(Debug)]
pub struct StatusBarModel {
    is_loading: bool,
    text: String,
    duration: String,
    start_marker: Option<Instant>,
}

#[derive(Debug)]
pub enum StatusBarMsg {
    Start,
    StopWithInfo { text: Option<String> },
}

#[relm4::component(pub)]
impl SimpleComponent for StatusBarModel {
    type Widgets = StatusBarWidgets;
    type Init = ();
    type Input = StatusBarMsg;
    type Output = ();

    view! {
      gtk::CenterBox {
        set_orientation: gtk::Orientation::Horizontal,
        set_margin_all: 10,
        set_hexpand: true,
        #[wrap(Some)]
        set_start_widget = &gtk::Box {
          gtk::Spinner {
            #[watch]
            set_spinning: model.is_loading,
          },

          gtk::Label {
            #[watch]
            set_label: model.text.as_str(),
          },
        },
        #[wrap(Some)]
        set_end_widget = &gtk::Box {
          gtk::Label {
            set_halign: gtk::Align::End,
            #[watch]
            set_label: model.duration.as_str(),
          },
        },
      }
    }

    fn init(_: (), root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = StatusBarModel {
            is_loading: false,
            text: String::default(),
            duration: String::default(),
            start_marker: None,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            StatusBarMsg::Start => {
                self.is_loading = true;
                self.text = String::default();
                self.duration = String::default();
                self.start_marker = Some(Instant::now());
            }
            StatusBarMsg::StopWithInfo { text } => {
                self.is_loading = false;
                self.text = text.unwrap_or_default();
                self.duration = format!(
                    "Took {:?}",
                    self.start_marker
                        .unwrap_or_else(|| Instant::now())
                        .elapsed()
                );
            }
        }
    }
}
