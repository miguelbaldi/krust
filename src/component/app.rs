//! Application entrypoint.

use gtk::prelude::*;
use relm4::{factory::FactoryVecDeque, prelude::*};
use tracing::{info, warn};

use crate::{component::{application_header::HeaderOutput, connection_list::ConnectionOutput}, config::State};

use super::{application_header::HeaderModel, connection_list::Connection};

#[derive(Debug)]
struct DialogModel {
  hidden: bool,
}

#[derive(Debug)]
enum DialogInput {
  Show,
  Accept,
  Cancel,
}

#[derive(Debug)]
enum DialogOutput {
  Close,
}

#[relm4::component]
impl SimpleComponent for DialogModel {
  type Init = bool;
  type Input = DialogInput;
  type Output = DialogOutput;
  
  view! {
    gtk::MessageDialog {
      set_modal: true,
      #[watch]
      set_visible: !model.hidden,
      set_text: Some("Do you want to close before saving?"),
      set_secondary_text: Some("All unsaved changes will be lost"),
      add_button: ("Close", gtk::ResponseType::Accept),
      add_button: ("Cancel", gtk::ResponseType::Cancel),
      connect_response[sender] => move |_, resp| {
        sender.input(if resp == gtk::ResponseType::Accept {
          DialogInput::Accept
        } else {
          DialogInput::Cancel
        })
      }
    }
  }
  
  fn init(
    params: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let model = DialogModel { hidden: params };
    let widgets = view_output!();
    ComponentParts { model, widgets }
  }
  
  fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
      DialogInput::Show => {
        self.hidden = false;
      }
      DialogInput::Accept => {
        self.hidden = true;
        sender.output(DialogOutput::Close).unwrap()
      }
      DialogInput::Cancel => self.hidden = true,
    }
  }
}

#[derive(Debug)]
pub enum AppMode {
  View,
  Edit,
  Export,
}

#[derive(Debug)]
pub enum AppMsg {
  SetMode(AppMode),
  CloseRequest(State),
  Close,
  AddConnection(Connection),
  RemoveConnection(DynamicIndex),
}

#[derive(Debug)]
pub struct AppModel {
  mode: AppMode,
  state: State,
  header: Controller<HeaderModel>,
  dialog: Controller<DialogModel>,
  connections: FactoryVecDeque<Connection>,
}

#[relm4::component(pub)]
impl Component for AppModel {
  type Init = AppMode;
  type Input = AppMsg;
  type Output = ();
  type CommandOutput = ();
  
  view! {
    main_window = gtk::Window {
      set_maximized: model.state.is_maximized,
      set_default_size: (model.state.width, model.state.height),
      set_titlebar: Some(model.header.widget()),
      #[name(main_paned)]
      gtk::Paned {
        set_orientation: gtk::Orientation::Horizontal,
        set_resize_start_child: true,
        set_position: ((model.state.width as f32) * 0.35).round() as i32,
        #[wrap(Some)]
        set_start_child = &gtk::ScrolledWindow {
          set_min_content_width: 200,
          set_hexpand: true,
          set_vexpand: true,
          set_propagate_natural_width: true,
          #[local_ref]
          connection_listbox -> gtk::ListBox {
              set_selection_mode: gtk::SelectionMode::Single,
              set_hexpand: true,
              set_vexpand: true,
              set_show_separators: true,
            },
        },
        #[wrap(Some)]
        set_end_child = &gtk::ScrolledWindow {
          set_hexpand: true,
          set_vexpand: true,
          #[wrap(Some)]
          set_child = &gtk::Label {
            #[watch]
            set_label: &format!("Placeholder for {:?}", model.mode),
          },
        },
      },
      
      connect_close_request[sender] => move |this| {
        let (width, height) = this.default_size();
        let is_maximized = this.is_maximized();
        
        let new_state = State {
          width,
          height,
          is_maximized,
        };
        
        sender.input(AppMsg::CloseRequest(new_state));
        gtk::glib::Propagation::Stop
      }
    }
  }
  
  fn init(
    params: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let header: Controller<HeaderModel> =
    HeaderModel::builder()
    .launch(())
    .forward(sender.input_sender(), |msg| match msg {
      HeaderOutput::View => AppMsg::SetMode(AppMode::View),
      HeaderOutput::Edit => AppMsg::SetMode(AppMode::Edit),
      HeaderOutput::Export => AppMsg::SetMode(AppMode::Export),
      HeaderOutput::AddConnection => AppMsg::AddConnection(Connection { name: "Foo connection".into() }),
    });
    
    let dialog = DialogModel::builder()
    .transient_for(&root)
    .launch(true)
    .forward(sender.input_sender(), |msg| match msg {
      DialogOutput::Close => AppMsg::Close,
    });
    
    let state = State::read()
    .map_err(|e| {
      warn!("unable to read application state: {}", e);
      e
    })
    .unwrap_or_default();
    
    let connections = FactoryVecDeque::builder()
    .launch(gtk::ListBox::default())
    .forward(sender.input_sender(), |output| match output {
      ConnectionOutput::Add(conn) => AppMsg::AddConnection(conn),
      ConnectionOutput::Remove(index) => AppMsg::RemoveConnection(index),
    });
    
    info!("starting with application state: {:?}", state);
    let model = AppModel {
      mode: params,
      state,
      header,
      dialog,
      connections
    };
    let connection_listbox = model.connections.widget();
    let widgets = view_output!();
    ComponentParts { model, widgets }
  }
  
  fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _: &Self::Root) {
    match msg {
      AppMsg::SetMode(mode) => {
        self.mode = mode;
      }
      AppMsg::CloseRequest(state) => {
        self.state = state;
        self.dialog.sender().send(DialogInput::Show).unwrap();
      }
      AppMsg::Close => {
        if let Err(e) = self.state.write() {
          warn!("unable to write application state: {}", e);
        }
        relm4::main_application().quit();
      }
      AppMsg::AddConnection(conn) => {
        info!("Adding connection {:?}", conn);
        self.connections.guard().push_front(conn.name);
      }
      AppMsg::RemoveConnection(index) => {
        info!("Removing connection {:?}", index);
      }
    }
  }
  fn post_view(&self, widgets: &mut Self::Widgets) {
    if self.state.is_maximized {
      info!("should maximize");
      widgets.main_window.maximize();
    };
    // let position = widgets.main_window.default_size().0 / 2;
    // info!("paned position before {}", position);
    // widgets.main_paned.set_position(position);
    // info!("paned position after {}", widgets.main_paned.position());
  }
}
