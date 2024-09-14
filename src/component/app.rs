//! Application entrypoint.

use std::{collections::HashMap, time::Duration};

use adw::{prelude::*, Toast};
use gtk::glib;
use relm4::{
    abstractions::Toaster,
    actions::{RelmAction, RelmActionGroup},
    factory::FactoryVecDeque,
    main_adw_application, main_application,
    prelude::*,
    MessageBroker,
};
use relm4_components::alert::{Alert, AlertMsg, AlertResponse, AlertSettings};
use tracing::*;

use crate::{
    backend::repository::{KrustConnection, KrustTopic, Repository},
    component::{
        connection_list::{KrustConnectionMsg, KrustConnectionOutput},
        connection_page::{ConnectionPageModel, ConnectionPageMsg, ConnectionPageOutput},
        settings_dialog::{SettingsDialogInit, SettingsDialogMsg},
        status_bar::{StatusBarModel, STATUS_BROKER},
        task_manager::{TaskManagerModel, TASK_MANAGER_BROKER},
        topics::topics_page::{TopicsPageMsg, TopicsPageOutput},
    },
    config::State,
    modals::about::AboutDialog,
    APP_ID, APP_NAME, APP_RESOURCE_PATH,
};

use super::{
    connection_list::ConnectionListModel,
    messages::messages_page::{MessagesPageModel, MessagesPageMsg},
    settings_dialog::SettingsDialogModel,
    topics::topics_page::TopicsPageModel,
};

#[derive(Debug)]
pub enum AppMsg {
    CloseRequest,
    Close,
    CloseIgnore,
    AddConnection(KrustConnection),
    ShowConnection,
    SaveConnection(Option<DynamicIndex>, KrustConnection),
    ShowEditConnectionPage(DynamicIndex, KrustConnection),
    ShowTopicsPage(KrustConnection),
    ShowTopicsPageByIndex(i32),
    ShowMessagesPage(KrustConnection, KrustTopic),
    HandleTopicsError(KrustConnection, bool),
    RemoveConnection(DynamicIndex, KrustConnection),
    ShowSettings,
    SavedSettings,
    ShowToast(String, String),
    HideToast(String),
}

#[derive(Debug)]
pub enum AppCommand {
    LateHide(String),
}

pub struct AppModel {
    toaster: Toaster,
    toasts: HashMap<String, Toast>,
    _status_bar: Controller<StatusBarModel>,
    _task_manager: Controller<TaskManagerModel>,
    close_dialog: Controller<Alert>,
    _about_dialog: Controller<AboutDialog>,
    connections: FactoryVecDeque<ConnectionListModel>,
    connection_page: Controller<ConnectionPageModel>,
    topics_page: Controller<TopicsPageModel>,
    messages_page: Controller<MessagesPageModel>,
    settings_dialog: Controller<SettingsDialogModel>,
}

relm4::new_action_group!(pub(super) WindowActionGroup, "win");
relm4::new_stateless_action!(pub(super) EditSettings, WindowActionGroup, "edit-settings");
relm4::new_stateless_action!(pub(super) AddConnection, WindowActionGroup, "add-connection");
relm4::new_stateless_action!(pub(super) ShortcutsAction, WindowActionGroup, "show-help-overlay");
relm4::new_stateless_action!(AboutAction, WindowActionGroup, "about");

pub static TOASTER_BROKER: MessageBroker<AppMsg> = MessageBroker::new();

#[relm4::component(pub)]
impl Component for AppModel {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = AppCommand;

    menu! {
        primary_menu: {
            section! {
                "_Settings" => EditSettings,
                "_Add connection" => AddConnection,
                "_Keyboard" => ShortcutsAction,
                "_About" => AboutAction,
            }
        }
    }

    view! {
        main_window = adw::ApplicationWindow::new(&main_adw_application()) {
            set_visible: true,
            set_title: Some(APP_NAME),
            set_icon_name: Some(APP_ID),
            set_width_request: 380,
            set_height_request: 380,
            set_default_size: (800, 600),
            set_show_menubar: true,
            #[local_ref]
            toast_overlay -> adw::ToastOverlay {
                #[name = "main_paned"]
                adw::OverlaySplitView {
                    set_enable_show_gesture: false,
                    set_enable_hide_gesture: false,
                    set_max_sidebar_width: 600.0,
                    //set_min_sidebar_width: 410.0,
                    set_sidebar_width_unit: adw::LengthUnit::Px,
                    set_sidebar_width_fraction: 0.2136,
                    #[wrap(Some)]
                    set_sidebar = &adw::ToolbarView {
                        add_top_bar = &adw::HeaderBar {
                            set_show_title: true,
                            #[wrap(Some)]
                            set_title_widget = &adw::WindowTitle {
                                set_title: "Connections",
                            },
                            pack_start = &gtk::ToggleButton {
                                set_icon_name: "list-add-symbolic",
                                set_tooltip_text: Some("Add a new kafka connection"),
                                set_action_name: Some("win.add-connection"),
                            },
                            pack_end = &gtk::MenuButton {
                                set_icon_name: "open-menu-symbolic",
                                set_menu_model: Some(&primary_menu),
                            }
                        },
                        #[wrap(Some)]
                        set_content = &gtk::ScrolledWindow {
                            set_min_content_width: 200,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_propagate_natural_width: true,
                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                gtk::StackSwitcher {
                                    set_overflow: gtk::Overflow::Hidden,
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_stack: Some(&main_stack),
                                    set_hexpand: false,
                                    set_vexpand: false,
                                    set_valign: gtk::Align::Baseline,
                                    set_halign: gtk::Align::Center,
                                },
                                gtk::ScrolledWindow {
                                    connections.widget() -> &gtk::ListBox {
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
                                task_manager.widget() -> &adw::Bin {},
                            },
                        },
                    },
                    #[wrap(Some)]
                    set_content = &adw::ToolbarView {
                        add_top_bar = &adw::HeaderBar {
                            pack_start: toggle_pane_button = &gtk::ToggleButton {
                                set_icon_name: "sidebar-show-symbolic",
                                set_active: true,
                                set_visible: false,
                            }
                        },
                        #[wrap(Some)]
                        set_content = &gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                #[name(main_stack)]
                                gtk::Stack {
                                    add_child = &gtk::Box {
                                        set_halign: gtk::Align::Center,
                                        set_orientation: gtk::Orientation::Vertical,
                                        #[name="support_logo"]
                                        gtk::Picture {
                                            set_vexpand: true,
                                            set_hexpand: true,
                                            set_margin_top: 48,
                                            set_margin_bottom: 48,
                                        },
                                    } -> {
                                        set_title: "Home",
                                        set_name: "Home",
                                    },
                                    add_child = topics_page.widget() -> &adw::TabOverview {} -> {
                                        set_name: "Topics",
                                        set_title: "Topics",
                                    },
                                    add_child = messages_page.widget() -> &adw::TabOverview {} -> {
                                        set_name: "Messages",
                                        set_title: "Messages",
                                    },
                                },
                            },
                        },
                    }
                },
            },

            connect_close_request[sender] => move |_this| {
                sender.input(AppMsg::Close);
                gtk::glib::Propagation::Stop
            },
        }
    }

    fn init(_params: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let toaster = Toaster::default();
        let toast_overlay = toaster.overlay_widget();
        let about_dialog = AboutDialog::builder()
            .transient_for(&root)
            .launch(())
            .detach();

        let status_bar: Controller<StatusBarModel> = StatusBarModel::builder()
            .launch_with_broker((), &STATUS_BROKER)
            .detach();

        let task_manager: Controller<TaskManagerModel> = TaskManagerModel::builder()
            .launch_with_broker((), &TASK_MANAGER_BROKER)
            .detach();

        let connections = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .forward(sender.input_sender(), |output| match output {
                KrustConnectionOutput::Add => AppMsg::ShowConnection,
                KrustConnectionOutput::Remove(index, conn) => AppMsg::RemoveConnection(index, conn),
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
                TopicsPageOutput::HandleError(conn, disconnect) => {
                    AppMsg::HandleTopicsError(conn, disconnect)
                }
            });

        let messages_page: Controller<MessagesPageModel> = MessagesPageModel::builder()
            .priority(glib::Priority::HIGH_IDLE)
            .launch(())
            .detach();

        let settings_dialog: Controller<SettingsDialogModel> = SettingsDialogModel::builder()
            .launch(SettingsDialogInit {})
            .detach();

        let state = State::read().unwrap_or_default();
        info!("starting with application state: {:?}", &state);
        let widgets = view_output!();
        info!("widgets loaded");
        widgets
            .support_logo
            .set_resource(Some(format!("{}logo.png", APP_RESOURCE_PATH).as_str()));

        let mut actions = RelmActionGroup::<WindowActionGroup>::new();

        let settings_sender = sender.clone();
        let edit_settings_action = RelmAction::<EditSettings>::new_stateless(move |_| {
            settings_sender.input(AppMsg::ShowSettings);
        });

        let input_sender = sender.clone();
        let add_connection_action = RelmAction::<AddConnection>::new_stateless(move |_| {
            input_sender.input(AppMsg::ShowConnection);
        });

        let about_sender = about_dialog.sender().clone();
        let about_action = RelmAction::<AboutAction>::new_stateless(move |_| {
            about_sender.send(()).unwrap();
        });
        info!("adding actions to main windows");
        actions.add_action(edit_settings_action);
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
        let model = AppModel {
            toaster,
            toasts: HashMap::new(),
            _status_bar: status_bar,
            _task_manager: task_manager,
            close_dialog: Alert::builder()
                .transient_for(&root)
                .launch(AlertSettings {
                    text: String::from("Do you want to close before saving?"),
                    secondary_text: Some(String::from("All unsaved changes will be lost")),
                    confirm_label: Some(String::from("Close")),
                    cancel_label: Some(String::from("Cancel")),
                    option_label: None,
                    is_modal: true,
                    destructive_accept: true,
                })
                .forward(sender.input_sender(), convert_alert_response),
            _about_dialog: about_dialog,
            connections,
            //main_stack: main_stk,
            connection_page,
            topics_page,
            messages_page,
            settings_dialog,
        };
        widgets.load_window_size();
        // DEBUG: start
        let main_window = main_application().active_window().unwrap();
        let surface = main_window.surface();
        if let Some(surface) = surface {
            let toplevel = surface.downcast::<gtk::gdk::Toplevel>();
            if let Ok(toplevel) = toplevel {
                info!("TOPLEVEL::{:?}", toplevel);
                toplevel.connect_layout(|_surface, _width, _height| {
                    //trace!("TOPLEVEL::LAYOUT::[width={},height={}]", width, height);
                });
                toplevel.connect_enter_monitor(|_surface, monitor| {
                    //trace!("TOPLEVEL::ENTER_MONITOR::[monitor={:?}]", monitor);
                    if let Some(_description) = monitor.description() {
                        //trace!("TOPLEVEL::ENTER_MONITOR::[description={}]", description.to_string());
                    }
                    if let Some(_connector) = monitor.connector() {
                        //trace!("TOPLEVEL::ENTER_MONITOR::[connector={}]", connector.to_string());
                    }
                });
                toplevel.connect_leave_monitor(|_surface, monitor| {
                    //trace!("TOPLEVEL::LEAVE_MONITOR::[monitor={:?}]", monitor);
                    if let Some(_description) = monitor.description() {
                        //trace!("TOPLEVEL::LEAVE_MONITOR::[description={}]", description.to_string());
                    }
                    if let Some(_connector) = monitor.connector() {
                        // trace!("TOPLEVEL::LEAVE_MONITOR::[connector={}]", connector.to_string());
                    }
                });
                // toplevel.connect_compute_size(|tl, cs| {
                //     let bounds = cs.bounds();
                //     info!("TOPLEVEL::BOUNDS::{:?}", bounds);
                // });
            }
        };
        // DEBUG: end
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
            AppMsg::ShowToast(id, text) => {
                let toast = adw::Toast::builder().title(text).timeout(0).build();
                self.toasts.insert(id, toast.clone());
                self.toaster.add_toast(toast);
            }
            AppMsg::HideToast(id) => {
                info!("hide_toast::{}", &id);
                let command_sender = sender.command_sender().clone();
                gtk::glib::timeout_add_once(Duration::from_secs(1), move || {
                    command_sender.emit(AppCommand::LateHide(id));
                });
            }
            AppMsg::CloseIgnore => (),
            AppMsg::CloseRequest => {
                self.close_dialog.emit(AlertMsg::Show);
            }
            AppMsg::Close => {
                relm4::main_application().quit();
            }
            AppMsg::ShowConnection => {
                info!("|-->Showing new connection page");
                self.connection_page.emit(ConnectionPageMsg::New);
                self.connection_page.widget().set_visible(true);
                //widgets.main_stack.set_visible_child_name("Connection");
            }
            AppMsg::AddConnection(conn) => {
                info!("|-->Adding connection ");

                self.connections.guard().push_back(conn);
            }
            AppMsg::SaveConnection(maybe_idx, conn) => {
                info!("|-->Saving connection {:?}", conn);

                //widgets.main_stack.set_visible_child_name("Home");
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
                                conn_to_update.color = new_conn.color;
                                conn_to_update.timeout = new_conn.timeout;
                            }
                            None => warn!("no connection to update"),
                        };
                    }
                    (_, Err(e)) => {
                        error!("error saving connection: {:?}", e);
                    }
                };
                self.connections.broadcast(KrustConnectionMsg::Refresh);
            }
            AppMsg::ShowEditConnectionPage(index, conn) => {
                info!("|-->Show edit connection page for {:?}", conn);
                self.connection_page
                    .emit(ConnectionPageMsg::Edit(index, conn));
                //widgets.main_stack.set_visible_child_name("Connection");
            }
            AppMsg::ShowTopicsPage(conn) => {
                info!("|-->Show edit connection page for {:?}", conn);
                self.topics_page.emit(TopicsPageMsg::Open(conn));
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
                    self.topics_page.emit(TopicsPageMsg::Open(conn));
                    widgets.main_stack.set_visible_child_name("Topics");
                } else {
                    widgets.main_stack.set_visible_child_name("Home");
                }
            }
            AppMsg::RemoveConnection(index, conn) => {
                info!("Removing connection {:?}::{:?}", index, conn);
                let mut repo = Repository::new();
                let result = repo.delete_connection(conn.id.unwrap());
                match result {
                    Ok(_) => {
                        self.connections.guard().remove(index.current_index());
                    }
                    Err(e) => {
                        error!("error saving connection: {:?}", e);
                    }
                };
            }
            AppMsg::ShowMessagesPage(connection, topic) => {
                self.messages_page
                    .emit(MessagesPageMsg::Open(Box::new(connection), Box::new(topic)));
                widgets.main_stack.set_visible_child_name("Messages");
            }
            AppMsg::SavedSettings => {
                widgets.main_stack.set_visible_child_name("Home");
            }
            AppMsg::ShowSettings => {
                info!("|-->Showing settings dialog");
                self.settings_dialog.emit(SettingsDialogMsg::Show);
            }
            AppMsg::HandleTopicsError(conn, disconnect) => {
                self.topics_page.emit(TopicsPageMsg::MenuPageClosed);
                if disconnect {
                    for c in self.connections.guard().iter_mut() {
                        info!("Looking for connections::{}={:?}", conn.name, c);
                        if c.name == conn.name {
                            c.is_connected = false;
                            break;
                        }
                    }
                    self.connections.broadcast(KrustConnectionMsg::Refresh);
                };
            }
        }
        self.update_view(widgets, sender);
    }

    fn update_cmd_with_view(
        &mut self,
        _widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppCommand::LateHide(id) => {
                if let Some(toast) = self.toasts.remove(&id) {
                    info!("hide_toast::removed::{}", &id);
                    toast.dismiss();
                }
            }
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        info!("app::saving window state");
        widgets
            .save_window_size()
            .expect("window state should be saved");
    }
}

fn convert_alert_response(response: AlertResponse) -> AppMsg {
    match response {
        AlertResponse::Confirm => AppMsg::Close,
        AlertResponse::Cancel => AppMsg::CloseIgnore,
        AlertResponse::Option => AppMsg::CloseIgnore,
    }
}

impl AppModelWidgets {
    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let (width, height) = self.main_window.default_size();
        let is_maximized = self.main_window.is_maximized();
        // let separator = if self.main_paned.position() < 405 {
        //     405
        // } else {
        //     self.main_paned.position()
        // };
        let new_state = State {
            width,
            height,
            separator_position: 300,
            is_maximized,
        };

        if let Err(e) = new_state.write() {
            warn!("unable to write application state: {}", e);
        }

        Ok(())
    }

    fn load_window_size(&self) {
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1510.0,
            adw::LengthUnit::Px,
        ));
        breakpoint.add_setter(&self.main_paned, "collapsed", &true.to_value());
        breakpoint.add_setter(&self.main_paned, "enable-show-gesture", &true.to_value());
        breakpoint.add_setter(&self.main_paned, "enable-hide-gesture", &true.to_value());
        breakpoint.add_setter(&self.toggle_pane_button, "visible", &true.to_value());
        let _toggle_sidebar_binding = self
            .toggle_pane_button
            .bind_property("active", &self.main_paned, "show-sidebar")
            .bidirectional()
            .sync_create()
            .build();
        self.main_window.add_breakpoint(breakpoint);
        info!("loading window size");
        let state = State::read()
            .map_err(|e| {
                warn!("unable to read application state: {}", e);
                e
            })
            .unwrap_or_default();
        let width = &state.width;
        let height = &state.height;
        let _paned_position = &state.separator_position;
        let is_maximized = &state.is_maximized;

        self.main_window.set_default_size(*width, *height);
        //self.main_paned.set_position(*paned_position);

        if *is_maximized {
            info!("should maximize");
            self.main_window.maximize();
        };
        info!("window size loaded");
    }
}
