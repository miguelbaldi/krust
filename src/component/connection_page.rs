use gtk::{prelude::{
  ButtonExt, ToggleButtonExt, WidgetExt, OrientableExt,
}};
use relm4::*;

#[derive(Debug)]
pub struct ConnectionPageModel{
  name: String,
}

#[derive(Debug)]
pub enum ConnectionPageOutput {
  Add,
  Save,
}

#[relm4::component(pub)]
impl SimpleComponent for ConnectionPageModel {
  type Init = ();
  type Input = ();
  type Output = ConnectionPageOutput;

  view! {
      #[root]
      gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            gtk::Label {
              set_label: "Name"
            },
            gtk::Entry {
              
            }
          }
          
      }
  }

  fn init(
      _params: Self::Init,
      root: Self::Root,
      sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
      let model = ConnectionPageModel { name: "".into()};
      let widgets = view_output!();
      ComponentParts { model, widgets }
  }
}