use gtk::prelude::{
  ButtonExt, ToggleButtonExt, WidgetExt,
};
use relm4::*;

#[derive(Debug)]
pub struct HeaderModel;

#[derive(Debug)]
pub enum HeaderOutput {
  View,
  Edit,
  Export,
  AddConnection,
}


#[relm4::component(pub)]
impl SimpleComponent for HeaderModel {
  type Init = ();
  type Input = ();
  type Output = HeaderOutput;

  view! {
      #[root]
      gtk::HeaderBar {
          #[wrap(Some)]
          set_title_widget = &gtk::Box {
              add_css_class: "linked",
              #[name = "group"]
              gtk::ToggleButton {
                  set_label: "View",
                  set_active: true,
                  connect_toggled[sender] => move |btn| {
                      if btn.is_active() {
                          sender.output(HeaderOutput::View).unwrap()
                      }
                  },
              },
              gtk::ToggleButton {
                  set_label: "Edit",
                  set_group: Some(&group),
                  connect_toggled[sender] => move |btn| {
                      if btn.is_active() {
                          sender.output(HeaderOutput::Edit).unwrap()
                      }
                  },
              },
              gtk::ToggleButton {
                  set_label: "Export",
                  set_group: Some(&group),
                  connect_toggled[sender] => move |btn| {
                      if btn.is_active() {
                          sender.output(HeaderOutput::Export).unwrap()
                      }
                  },
              },
              gtk::Button {
                set_label: "Add connection",
                connect_clicked[sender] => move |_btn| {
                  sender.output(HeaderOutput::AddConnection).unwrap()
                },
              },
          }
      }
  }

  fn init(
      _params: Self::Init,
      root: Self::Root,
      sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
      let model = HeaderModel;
      let widgets = view_output!();
      ComponentParts { model, widgets }
  }
}