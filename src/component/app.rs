//! Application entrypoint.

use gtk::prelude::*;
use relm4::{actions::{RelmAction, RelmActionGroup}, factory::FactoryVecDeque, main_application, prelude::*};
use tracing::{error, info, warn};

use crate::{
  backend::repository::{KrustConnection, KrustMessage, Repository}, component::{
    application_header::HeaderOutput, connection_list::KrustConnectionOutput, connection_page::{ConnectionPageModel, ConnectionPageMsg, ConnectionPageOutput}, messages_page::{MessagesPageModel, MessagesPageOutput}, topics_page::{TopicsPageModel, TopicsPageMsg, TopicsPageOutput}
  }, config::State, modals::about::AboutDialog
};

use super::{application_header::HeaderModel, connection_list::ConnectionListModel, messages_page::MessagesPageMsg};

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
    #[name(message_dialog)]
    gtk::MessageDialog {
      set_modal: true,
      set_default_height: 160,
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
pub enum AppMsg {
  CloseRequest(State),
  Close,
  AddConnection(KrustConnection),
  ShowConnection,
  SaveConnection(Option<DynamicIndex>, KrustConnection),
  ShowEditConnectionPage(DynamicIndex, KrustConnection),
  ShowTopicsPage(KrustConnection),
  ShowMessagesPage(Vec<KrustMessage>),
  RemoveConnection(DynamicIndex),
}

#[derive(Debug)]
pub struct AppModel {
  state: State,
  _header: Controller<HeaderModel>,
  dialog: Controller<DialogModel>,
  _about_dialog: Controller<AboutDialog>,
  connections: FactoryVecDeque<ConnectionListModel>,
  main_stack: gtk::Stack,
  connection_page: Controller<ConnectionPageModel>,
  topics_page: Controller<TopicsPageModel>,
  messages_page: Controller<MessagesPageModel>,
}

relm4::new_action_group!(pub(super) WindowActionGroup, "win");
relm4::new_stateless_action!(pub(super) AddConnection, WindowActionGroup, "add-connection");
relm4::new_stateless_action!(pub(super) ShortcutsAction, WindowActionGroup, "show-help-overlay");
relm4::new_stateless_action!(AboutAction, WindowActionGroup, "about");

#[relm4::component(pub)]
impl Component for AppModel {
  type Init = ();
  type Input = AppMsg;
  type Output = ();
  type CommandOutput = ();
  
  menu! {
    primary_menu: {
      section! {
        "_Add connection" => AddConnection,
        "_Keyboard" => ShortcutsAction,
        "_About" => AboutAction,
      }
    }
  }
  
  view! {
    main_window = adw::ApplicationWindow::new(&main_application()) {
      set_visible: true,
      set_maximized: state.is_maximized,
      set_default_size: (state.width, state.height),
      //set_titlebar: Some(header.widget()),
      set_title: Some("KRust Kafka Client"),
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,
        
        adw::HeaderBar {
          pack_end = &gtk::MenuButton {
            set_icon_name: "open-menu-symbolic",
            set_menu_model: Some(&primary_menu),
          }
        },
        #[name(main_paned)]
        gtk::Paned {
          set_orientation: gtk::Orientation::Horizontal,
          set_resize_start_child: true,
          set_position: state.separator_position,
          #[wrap(Some)]
          set_start_child = &gtk::ScrolledWindow {
            set_min_content_width: 200,
            set_hexpand: true,
            set_vexpand: true,
            set_propagate_natural_width: true,
            #[wrap(Some)]
            set_child = connections.widget() -> &gtk::ListBox {
              set_selection_mode: gtk::SelectionMode::Single,
              set_hexpand: true,
              set_vexpand: true,
              set_show_separators: true,
              add_css_class: "rich-list",
            },
          },
          #[wrap(Some)]
          set_end_child = &gtk::ScrolledWindow {
            set_hexpand: true,
            set_vexpand: true,
            #[wrap(Some)]
            set_child = &gtk::Box {
              #[name(main_stack)]
              gtk::Stack {
                add_child = &gtk::Box {
                  #[name(app_mode_label)]
                  gtk::Label {
                    #[watch]
                    set_label: "Home",
                  },
                } -> {
                  set_title: "Home",
                  set_name: "Home",
                },
                add_child = connection_page.widget() -> &gtk::Grid {} -> {
                  set_name: "Connection"
                },
                add_child = topics_page.widget() -> &gtk::Box {} -> {
                  set_name: "Topics"
                },
                add_child = messages_page.widget() -> &gtk::Box {} -> {
                  set_name: "Messages"
                },
              }
            }
          },
        },
      },
      
      connect_close_request[sender, main_paned] => move |this| {
        let (width, height) = this.default_size();
        let is_maximized = this.is_maximized();
        let separator = main_paned.position();
        let new_state = State {
          width,
          height,
          separator_position: separator,
          is_maximized,
        };
        
        sender.input(AppMsg::CloseRequest(new_state));
        gtk::glib::Propagation::Stop
      },
      
    }
  }
  
  fn init(
    _params: (),
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let state = State::read()
    .map_err(|e| {
      warn!("unable to read application state: {}", e);
      e
    })
    .unwrap_or_default();
    
    let about_dialog = AboutDialog::builder()
    .transient_for(&root)
    .launch(())
    .detach();
    
    let header: Controller<HeaderModel> =
    HeaderModel::builder()
    .launch(())
    .forward(sender.input_sender(), |msg| match msg {
      HeaderOutput::AddConnection => AppMsg::ShowConnection,
    });
    
    let dialog = DialogModel::builder()
    .transient_for(&root)
    .launch(true)
    .forward(sender.input_sender(), |msg| match msg {
      DialogOutput::Close => AppMsg::Close,
    });
    
    let connections = FactoryVecDeque::builder()
    .launch(gtk::ListBox::default())
    .forward(sender.input_sender(), |output| match output {
      KrustConnectionOutput::Add => AppMsg::ShowConnection,
      KrustConnectionOutput::Remove(index) => AppMsg::RemoveConnection(index),
      KrustConnectionOutput::Edit(index, conn) => AppMsg::ShowEditConnectionPage(index, conn),
      KrustConnectionOutput::ShowTopics(conn) => AppMsg::ShowTopicsPage(conn),
    });
    
    let connection_page: Controller<ConnectionPageModel> = ConnectionPageModel::builder()
    .launch(None)
    .forward(sender.input_sender(), |msg| match msg {
      ConnectionPageOutput::Save(index, conn) => AppMsg::SaveConnection(index,conn),
    });
    
    let topics_page: Controller<TopicsPageModel> = TopicsPageModel::builder()
    .launch(None)
    .forward(sender.input_sender(), |msg| match msg {
      TopicsPageOutput::OpenMessagesPage(data) => AppMsg::ShowMessagesPage(data),
    });

    let messages_page: Controller<MessagesPageModel> = MessagesPageModel::builder()
    .launch(None)
    .detach();

    info!("starting with application state: {:?}", state);
    //let connection_listbox: gtk::ListBox = connections.widget();
    let widgets = view_output!();
    
    let mut actions = RelmActionGroup::<WindowActionGroup>::new();
    
    let add_connection_action = {
      let input_sender = sender.clone();
      RelmAction::<AddConnection>::new_stateless(move |_| {
        input_sender.input(AppMsg::ShowConnection);
      })
    };
    
    let about_action = {
      let about_sender = about_dialog.sender().clone();
      RelmAction::<AboutAction>::new_stateless(move |_| {
        about_sender.send(()).unwrap();
      })
    };
    
    actions.add_action(add_connection_action);
    actions.add_action(about_action);
    actions.register_for_widget(&widgets.main_window);
    
    let mut repo = Repository::new();
    let conn_list = repo.list_all_connections();
    match conn_list {
      Ok(list) => {
        for conn in list {
          sender.input(AppMsg::AddConnection(conn));
        }
      },
      Err(e) => error!("error loading connections: {:?}", e),
    }
    
    let model = AppModel {
      state,
      _header: header,
      dialog,
      _about_dialog: about_dialog,
      connections,
      main_stack: widgets.main_stack.to_owned(),
      connection_page,
      topics_page,
      messages_page,
    };

    ComponentParts { model, widgets }
  }
  
  fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _: &Self::Root) {
    match msg {
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
      AppMsg::ShowConnection => {
        info!("|-->Showing connection ");
        
        self.connection_page.widget().set_visible(true);
        self.main_stack.set_visible_child_name("Connection");
      }
      AppMsg::AddConnection(conn) => {
        info!("|-->Adding connection ");
        
        self.connections.guard().push_back(conn);
      }
      AppMsg::SaveConnection(maybe_idx, conn) => {
        info!("|-->Saving connection {:?}", conn);
        
        self.main_stack.set_visible_child_name("Home");
        let mut repo = Repository::new();
        let result = repo.save_connection(&conn);
        match (maybe_idx, result) {
          (None, Ok(new_conn)) => {
            self.connections.guard().push_back(new_conn);
          },
          (Some(idx), Ok(new_conn)) => {
            match self.connections.guard().get_mut(idx.current_index()) {
                Some(conn_to_update) => {
                  conn_to_update.name = new_conn.name;
                  conn_to_update.brokers_list = new_conn.brokers_list;
                  conn_to_update.security_type = new_conn.security_type;
                  conn_to_update.sasl_mechanism = new_conn.sasl_mechanism;
                  conn_to_update.jaas_config = new_conn.jaas_config;
                },
                None => todo!(),
            };
          },
          (_, Err(e)) => {error!("error saving connection: {:?}", e);},
        };
      }
      AppMsg::ShowEditConnectionPage(index, conn) => {
        info!("|-->Show edit connection page for {:?}", conn);
        self.connection_page.emit(ConnectionPageMsg::Edit(index, conn));
        self.main_stack.set_visible_child_name("Connection");
        
      }
      AppMsg::ShowTopicsPage(conn) => {
        info!("|-->Show edit connection page for {:?}", conn);
        self.topics_page.emit(TopicsPageMsg::List(conn));
        self.main_stack.set_visible_child_name("Topics");
        
      }
      AppMsg::RemoveConnection(index) => {
        info!("Removing connection {:?}", index);
      }
      AppMsg::ShowMessagesPage(messages) => {
        self.messages_page.emit(MessagesPageMsg::List(messages));
        self.main_stack.set_visible_child_name("Messages");
      }
    }
  }
  fn post_view(&self, widgets: &mut Self::Widgets) {
    if self.state.is_maximized {
      info!("should maximize");
      widgets.main_window.maximize();
    };
    
  }
}
