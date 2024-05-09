//! Application entrypoint.

use gtk::{glib, prelude::*};
use relm4::{
    actions::{RelmAction, RelmActionGroup},
    factory::FactoryVecDeque,
    main_adw_application,
    prelude::*,
};
use relm4_components::alert::{Alert, AlertMsg, AlertResponse, AlertSettings};
use tracing::{error, info, warn};

use crate::{
    backend::repository::{KrustConnection, KrustTopic, Repository},
    component::{
        banner::BANNER_BROKER, connection_list::KrustConnectionOutput, connection_page::{ConnectionPageModel, ConnectionPageMsg, ConnectionPageOutput}, settings_dialog::{SettingsDialogInit, SettingsDialogMsg}, status_bar::{StatusBarModel, STATUS_BROKER}, task_manager::{TaskManagerModel, TASK_MANAGER_BROKER}, topics::topics_page::{TopicsPageMsg, TopicsPageOutput}
    },
    config::State,
    modals::about::AboutDialog,
    APP_ID, APP_NAME, APP_RESOURCE_PATH,
};

use super::{
    banner::AppBannerModel, connection_list::ConnectionListModel, messages::messages_page::{MessagesPageModel, MessagesPageMsg}, settings_dialog::SettingsDialogModel, topics::topics_page::TopicsPageModel
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
    RemoveConnection(DynamicIndex),
    ShowSettings,
    SavedSettings,
}

pub struct AppModel {
    _app_banner: Controller<AppBannerModel>,
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

#[relm4::component(pub)]
impl Component for AppModel {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = ();

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
                    set_wide_handle: true,
                    #[wrap(Some)]
                    set_start_child = &gtk::ScrolledWindow {
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
                    #[wrap(Some)]
                    set_end_child = &gtk::ScrolledWindow {
                        set_hexpand: true,
                        set_vexpand: true,
                        #[wrap(Some)]
                        set_child = &gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            app_banner.widget() -> &adw::Banner {},
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
                                add_child = connection_page.widget() -> &gtk::Grid {} -> {
                                    set_name: "Connection",
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
                },
                gtk::Box {
                    set_visible: false,
                    add_css_class: "status-bar",
                    status_bar.widget() -> &gtk::CenterBox {}
                },
            },

            connect_close_request[sender] => move |_this| {
                sender.input(AppMsg::Close);
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

        let app_banner: Controller<AppBannerModel> = AppBannerModel::builder()
            .launch_with_broker((), &BANNER_BROKER)
            .detach();
        let task_manager: Controller<TaskManagerModel> = TaskManagerModel::builder()
            .launch_with_broker((), &TASK_MANAGER_BROKER)
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

        let messages_page: Controller<MessagesPageModel> = MessagesPageModel::builder()
            .priority(glib::Priority::HIGH_IDLE)
            .launch(())
            .detach();

        let settings_dialog: Controller<SettingsDialogModel> = SettingsDialogModel::builder()
            .launch(SettingsDialogInit{})
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
            _app_banner: app_banner,
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
            AppMsg::CloseIgnore => {
                ();
            }
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
                            None => warn!("no connection to update"),
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
            AppMsg::RemoveConnection(index) => {
                info!("Removing connection {:?}", index);
            }
            AppMsg::ShowMessagesPage(connection, topic) => {
                self.messages_page
                    .emit(MessagesPageMsg::Open(connection, topic));
                widgets.main_stack.set_visible_child_name("Messages");
            }
            AppMsg::SavedSettings => {
                widgets.main_stack.set_visible_child_name("Home");
            }
            AppMsg::ShowSettings => {
                info!("|-->Showing settings dialog");
                self.settings_dialog.emit(SettingsDialogMsg::Show);
            }
        }
        self.update_view(widgets, sender);
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
        let separator = if self.main_paned.position() < 405 {
            405
        } else {
            self.main_paned.position()
        };
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
