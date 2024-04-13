//! Application entrypoint.

use gtk::{gio, glib, prelude::*};
use relm4::{
    actions::{RelmAction, RelmActionGroup},
    factory::FactoryVecDeque,
    main_application,
    prelude::*,
};
use tracing::{error, info, warn};

use crate::{
    backend::{
        kafka::Topic,
        repository::{KrustConnection, Repository},
    },
    component::{
        connection_list::KrustConnectionOutput,
        connection_page::{ConnectionPageModel, ConnectionPageMsg, ConnectionPageOutput},
        status_bar::{StatusBarModel, STATUS_BROKER},
        topics_page::{TopicsPageModel, TopicsPageMsg, TopicsPageOutput},
    },
    config::State,
    modals::about::AboutDialog,
};

use super::{
    connection_list::ConnectionListModel,
    messages::messages_page::{MessagesPageModel, MessagesPageMsg},
};

#[derive(Debug)]
pub enum AppMsg {
    CloseRequest,
    Close,
    AddConnection(KrustConnection),
    ShowConnection,
    SaveConnection(Option<DynamicIndex>, KrustConnection),
    ShowEditConnectionPage(DynamicIndex, KrustConnection),
    ShowTopicsPage(KrustConnection),
    ShowTopicsPageByIndex(i32),
    ShowMessagesPage(KrustConnection, Topic),
    RemoveConnection(DynamicIndex),
}

#[derive(Debug)]
pub struct AppModel {
    //state: State,
    _status_bar: Controller<StatusBarModel>,
    //dialog: Controller<DialogModel>,
    _about_dialog: Controller<AboutDialog>,
    connections: FactoryVecDeque<ConnectionListModel>,
    //main_stack: gtk::Stack,
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
                connect_row_activated[sender] => move |list_box, row| {
                  info!("clicked on connection: {:?} - {:?}", list_box, row.index());
                  sender.input(AppMsg::ShowTopicsPageByIndex(row.index()));
                },
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
                  add_child = messages_page.widget() -> &gtk::Paned {} -> {
                    set_name: "Messages"
                  },
                }
              }
            },
          },
          gtk::Box {
            add_css_class: "status-bar",
            status_bar.widget() -> &gtk::CenterBox {}
          }
        },

        connect_close_request[sender] => move |_this| {
          sender.input(AppMsg::CloseRequest);
          gtk::glib::Propagation::Stop
        },

      }
    }

    fn init(_params: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let about_dialog = AboutDialog::builder()
            .transient_for(&root)
            .launch(())
            .detach();

        let status_bar: Controller<StatusBarModel> = StatusBarModel::builder()
            .launch_with_broker((), &STATUS_BROKER)
            .detach();

        let connections = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .forward(sender.input_sender(), |output| match output {
                KrustConnectionOutput::Add => AppMsg::ShowConnection,
                KrustConnectionOutput::Remove(index) => AppMsg::RemoveConnection(index),
                KrustConnectionOutput::Edit(index, conn) => {
                    AppMsg::ShowEditConnectionPage(index, conn)
                }
                KrustConnectionOutput::ShowTopics(conn) => AppMsg::ShowTopicsPage(conn),
            });

        let connection_page: Controller<ConnectionPageModel> = ConnectionPageModel::builder()
            .launch(None)
            .forward(sender.input_sender(), |msg| match msg {
                ConnectionPageOutput::Save(index, conn) => AppMsg::SaveConnection(index, conn),
            });

        let topics_page: Controller<TopicsPageModel> = TopicsPageModel::builder()
            .launch(None)
            .forward(sender.input_sender(), |msg| match msg {
                TopicsPageOutput::OpenMessagesPage(connection, topic) => {
                    AppMsg::ShowMessagesPage(connection, topic)
                }
            });

        let messages_page: Controller<MessagesPageModel> =
            MessagesPageModel::builder().launch(()).detach();

        let state = State::read().unwrap_or_default();
        info!("starting with application state: {:?}", &state);
        //let connection_listbox: gtk::ListBox = connections.widget();
        let widgets = view_output!();
        info!("widgets loaded");

        let mut actions = RelmActionGroup::<WindowActionGroup>::new();

        let input_sender = sender.clone();
        let add_connection_action = RelmAction::<AddConnection>::new_stateless(move |_| {
            input_sender.input(AppMsg::ShowConnection);
        });

        let about_sender = about_dialog.sender().clone();
        let about_action = RelmAction::<AboutAction>::new_stateless(move |_| {
            about_sender.send(()).unwrap();
        });
        info!("adding actions to main windows");
        actions.add_action(add_connection_action);
        actions.add_action(about_action);
        actions.register_for_widget(&widgets.main_window);

        info!("listing all connections");
        let mut repo = Repository::new();
        let conn_list = repo.list_all_connections();
        match conn_list {
            Ok(list) => {
                for conn in list {
                    sender.input(AppMsg::AddConnection(conn));
                }
            }
            Err(e) => error!("error loading connections: {:?}", e),
        }
        //let main_stk = widgets.main_stack;
        let model = AppModel {
            //state,
            _status_bar: status_bar,
            //dialog,
            _about_dialog: about_dialog,
            connections,
            //main_stack: main_stk,
            connection_page,
            topics_page,
            messages_page,
        };

        widgets.load_window_size();
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
            AppMsg::CloseRequest => {
                let close_alert = gtk::AlertDialog::builder()
                    .modal(true)
                    .message("Do you want to close before saving?")
                    .detail("All unsaved changes will be lost")
                    .buttons(["Close", "Cancel"])
                    .default_button(0)
                    .cancel_button(1)
                    .build();
                let alert_sender = sender.clone();
                close_alert.choose(
                    Some(&widgets.main_window),
                    None::<gio::Cancellable>.as_ref(),
                    move |r| match r {
                        Ok(0) => alert_sender.input(AppMsg::Close),
                        Ok(other) => info!("cancelled close {}", other),
                        Err(e) => warn!("problem on close alert: {}", e.to_string()),
                    },
                );
                close_alert.show(Some(&widgets.main_window));
            }
            AppMsg::Close => {
                relm4::main_application().quit();
            }
            AppMsg::ShowConnection => {
                info!("|-->Showing new connection page");
                self.connection_page.emit(ConnectionPageMsg::New);
                self.connection_page.widget().set_visible(true);
                widgets.main_stack.set_visible_child_name("Connection");
            }
            AppMsg::AddConnection(conn) => {
                info!("|-->Adding connection ");

                self.connections.guard().push_back(conn);
            }
            AppMsg::SaveConnection(maybe_idx, conn) => {
                info!("|-->Saving connection {:?}", conn);

                widgets.main_stack.set_visible_child_name("Home");
                let mut repo = Repository::new();
                let result = repo.save_connection(&conn);
                match (maybe_idx, result) {
                    (None, Ok(new_conn)) => {
                        self.connections.guard().push_back(new_conn);
                    }
                    (Some(idx), Ok(new_conn)) => {
                        match self.connections.guard().get_mut(idx.current_index()) {
                            Some(conn_to_update) => {
                                conn_to_update.name = new_conn.name;
                                conn_to_update.brokers_list = new_conn.brokers_list;
                                conn_to_update.security_type = new_conn.security_type;
                                conn_to_update.sasl_mechanism = new_conn.sasl_mechanism;
                                conn_to_update.sasl_username = new_conn.sasl_username;
                                conn_to_update.sasl_password = new_conn.sasl_password;
                            }
                            None => todo!(),
                        };
                    }
                    (_, Err(e)) => {
                        error!("error saving connection: {:?}", e);
                    }
                };
            }
            AppMsg::ShowEditConnectionPage(index, conn) => {
                info!("|-->Show edit connection page for {:?}", conn);
                self.connection_page
                    .emit(ConnectionPageMsg::Edit(index, conn));
                widgets.main_stack.set_visible_child_name("Connection");
            }
            AppMsg::ShowTopicsPage(conn) => {
                info!("|-->Show edit connection page for {:?}", conn);
                self.topics_page.emit(TopicsPageMsg::List(conn));
                widgets.main_stack.set_visible_child_name("Topics");
            }
            AppMsg::ShowTopicsPageByIndex(idx) => {
                let is_connected = self
                    .connections
                    .guard()
                    .get(idx as usize)
                    .unwrap()
                    .is_connected;
                if is_connected {
                    let conn: KrustConnection = self
                        .connections
                        .guard()
                        .get_mut(idx as usize)
                        .unwrap()
                        .into();
                    info!(
                        "|-->Show edit connection page for index {:?} - {:?}",
                        idx, conn
                    );
                    self.topics_page.emit(TopicsPageMsg::List(conn));
                    widgets.main_stack.set_visible_child_name("Topics");
                } else {
                    widgets.main_stack.set_visible_child_name("Home");
                }
            }
            AppMsg::RemoveConnection(index) => {
                info!("Removing connection {:?}", index);
            }
            AppMsg::ShowMessagesPage(connection, topic) => {
                self.messages_page
                    .emit(MessagesPageMsg::Open(connection, topic));
                widgets.main_stack.set_visible_child_name("Messages");
            }
        }
        self.update_view(widgets, sender);
    }
    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        widgets
            .save_window_size()
            .expect("window state should be saved");
    }
}

impl AppModelWidgets {
    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let (width, height) = self.main_window.default_size();
        let is_maximized = self.main_window.is_maximized();
        let separator = self.main_paned.position();
        let new_state = State {
            width,
            height,
            separator_position: separator,
            is_maximized,
        };

        if let Err(e) = new_state.write() {
            warn!("unable to write application state: {}", e);
        }

        Ok(())
    }

    fn load_window_size(&self) {
        info!("loading window size");
        let state = State::read()
            .map_err(|e| {
                warn!("unable to read application state: {}", e);
                e
            })
            .unwrap_or_default();
        let width = &state.width;
        let height = &state.height;
        let paned_position = &state.separator_position;
        let is_maximized = &state.is_maximized;

        self.main_window.set_default_size(*width, *height);
        self.main_paned.set_position(*paned_position);

        if *is_maximized {
            info!("should maximize");
            self.main_window.maximize();
        };
        info!("window size loaded");
    }
}
