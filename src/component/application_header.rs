use gtk::prelude::*;
use relm4::*;

#[derive(Debug)]
pub struct HeaderModel;

#[derive(Debug)]
pub enum HeaderOutput {
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
          set_title_widget = &gtk::Label {
              add_css_class: "title",
              add_css_class: "header-title",
              set_label: "KRust Kafka Client",
          },
          pack_end = &gtk::Box {
            gtk::Button {
              set_label: "Add connection",
              connect_clicked[sender] => move |_btn| {
                sender.output(HeaderOutput::AddConnection).unwrap()
              },
            },
        },
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