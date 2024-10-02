#![allow(deprecated)]

// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use crate::{
    backend::repository::{KrustConnection, KrustTopic},
    component::{colorize_widget_by_connection, get_tab_by_title},
    AppMsg, Repository, TOASTER_BROKER,
};
use adw::prelude::*;
use adw::TabPage;
use copypasta::{ClipboardContext, ClipboardProvider};
use relm4::{actions::RelmAction, factory::FactoryVecDeque, *};

use tracing::*;
use uuid::Uuid;

use super::messages_tab::{MessagesTabInit, MessagesTabModel, MessagesTabMsg};

relm4::new_action_group!(pub(super) TopicTabActionGroup, "topic-tab");
relm4::new_stateless_action!(pub(super) PinTabAction, TopicTabActionGroup, "toggle-pin");
relm4::new_stateless_action!(pub(super) CloseTabAction, TopicTabActionGroup, "close");
relm4::new_stateless_action!(pub(super) CopyTopicNameAction, TopicTabActionGroup, "copy-topic-name");

pub static MESSAGES_PAGE_BROKER: MessageBroker<MessagesPageMsg> = MessageBroker::new();

pub struct MessagesPageModel {
    topic: Option<KrustTopic>,
    connection: Option<KrustConnection>,
    topics: FactoryVecDeque<MessagesTabModel>,
    clipboard: Box<dyn ClipboardProvider>,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
    Open(Box<KrustConnection>, Box<KrustTopic>),
    PageAdded(i32),
    MenuPageClosed,
    MenuPagePin,
    CopyTopicName,
    RefreshTopicTab {
        connection_id: usize,
        topic_name: String,
    },
}

#[relm4::component(pub)]
impl Component for MessagesPageModel {
    type Init = ();
    type Input = MessagesPageMsg;
    type Output = ();
    type CommandOutput = ();

    menu! {
        tab_menu: {
            section! {
                "_Toggle pin" => PinTabAction,
                "_Close" => CloseTabAction,
                "_Copy topic name" => CopyTopicNameAction,
            }
        }
    }

    view! {
        #[root]
        adw::TabOverview {
            set_view: Some(topics_viewer),
            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                append: topics_tabs = &adw::TabBar {
                    set_autohide: false,
                    set_expand_tabs: true,
                    set_view: Some(topics_viewer),
                    #[wrap(Some)]
                    set_end_action_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        adw::TabButton {
                            set_view: Some(topics_viewer),
                            set_action_name: Some("overview.open"),
                        },
                    },
                },
                #[local_ref]
                topics_viewer -> adw::TabView {
                    set_menu_model: Some(&tab_menu),
                }
            },
        },

    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let topics = FactoryVecDeque::builder()
            .launch(adw::TabView::default())
            .detach();

        let topics_viewer: &adw::TabView = topics.widget();
        topics_viewer.connect_setup_menu(|view, page| {
            if let Some(page) = page {
                view.set_selected_page(page);
            }
        });
        let tabs_sender = sender.clone();
        topics_viewer.connect_page_attached(move |_tab_view, _page, n| {
            tabs_sender.input(MessagesPageMsg::PageAdded(n));
        });

        let widgets = view_output!();

        let mut topics_tabs_actions = relm4::actions::RelmActionGroup::<TopicTabActionGroup>::new();
        let tabs_sender = sender.input_sender().clone();
        let close_tab_action = RelmAction::<CloseTabAction>::new_stateless(move |_| {
            tabs_sender.send(MessagesPageMsg::MenuPageClosed).unwrap();
        });
        let tabs_sender = sender.input_sender().clone();
        let pin_tab_action = RelmAction::<PinTabAction>::new_stateless(move |_| {
            tabs_sender.send(MessagesPageMsg::MenuPagePin).unwrap();
        });
        let tabs_sender = sender.input_sender().clone();
        let copy_topic_name_action = RelmAction::<CopyTopicNameAction>::new_stateless(move |_| {
            tabs_sender.send(MessagesPageMsg::CopyTopicName).unwrap();
        });
        topics_tabs_actions.add_action(close_tab_action);
        topics_tabs_actions.add_action(pin_tab_action);
        topics_tabs_actions.add_action(copy_topic_name_action);
        topics_tabs_actions.register_for_widget(&widgets.topics_tabs);
        let clipboard = Box::new(ClipboardContext::new().unwrap());
        let model = MessagesPageModel {
            topic: None,
            connection: None,
            topics,
            clipboard,
        };

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: MessagesPageMsg,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match msg {
            MessagesPageMsg::Open(connection, topic) => {
                let title = format!("[{}] {}", connection.name, topic.name);
                let has_page: Option<(usize, TabPage)> = self.get_tab_page_by_title(widgets, title);
                match has_page {
                    Some((pos, page)) => {
                        info!(
                            "page already exists [position={}, tab={}]",
                            pos,
                            page.title()
                        );
                        widgets.topics_viewer.set_selected_page(&page);
                    }
                    None => {
                        info!("adding new page");
                        let conn_id = &connection.id.unwrap();
                        let topic_name = &topic.name.clone();
                        self.connection = Some(*connection);
                        let mut repo = Repository::new();
                        let maybe_topic = repo.find_topic(*conn_id, topic_name);
                        self.topic = maybe_topic.clone().or(Some(*topic));
                        let init = MessagesTabInit {
                            topic: self.topic.clone().unwrap(),
                            connection: self.connection.clone().unwrap(),
                        };
                        let _index = self.topics.guard().push_front(init);
                    }
                }
            }
            MessagesPageMsg::PageAdded(index) => {
                let tab_model = self.topics.get(index.try_into().unwrap()).unwrap();
                let conn = tab_model.connection.clone().unwrap();
                let title = format!("[{}] {}", conn.name, tab_model.topic.clone().unwrap().name);
                let page = self.get_tab_page_by_index(widgets, index);
                if let Some(page) = page {
                    page.set_title(title.as_str());
                    page.set_live_thumbnail(true);

                    let maybe_tab = get_tab_by_title(&widgets.topics_tabs, title);

                    if let Some(tab) = maybe_tab {
                        colorize_widget_by_connection(&conn, tab);
                    }

                    widgets.topics_viewer.set_selected_page(&page);
                }
            }
            MessagesPageMsg::MenuPagePin => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    let pinned = !page.is_pinned();
                    widgets.topics_viewer.set_page_pinned(&page, pinned);
                }
            }
            MessagesPageMsg::CopyTopicName => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    let topic = self.get_model_by_tab_page(page);
                    if let Some(topic) = topic {
                        let topic_name = topic.name.clone();
                        let id = Uuid::new_v4();
                        TOASTER_BROKER
                            .send(AppMsg::ShowToast(id.to_string(), "Copied!".to_string()));
                        self.clipboard
                            .set_contents(topic_name)
                            .unwrap_or_else(|err| {
                                warn!("unable to store topic name in clipboard: {}", err);
                            });
                        TOASTER_BROKER.send(AppMsg::HideToast(id.to_string()));
                    }
                }
            }
            MessagesPageMsg::MenuPageClosed => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    info!("closing messages page with name {}", page.title());
                    let mut idx: Option<usize> = None;
                    let mut topics = self.topics.guard();
                    for i in 0..topics.len() {
                        let tp = topics.get_mut(i);
                        if let Some(tp) = tp {
                            let title = format!(
                                "[{}] {}",
                                tp.connection.clone().unwrap().name.clone(),
                                tp.topic.clone().unwrap().name.clone()
                            );
                            info!("PageClosed [{}][{}={}]", i, title, page.title());
                            if title.eq(&page.title().to_string()) {
                                idx = Some(i);
                                break;
                            }
                        }
                    }
                    if let Some(idx) = idx {
                        let result = topics.remove(idx);
                        let name = if let Some(res) = result {
                            res.topic.unwrap().name
                        } else {
                            String::new()
                        };
                        info!("page model with index {} and name {} removed", idx, name);
                    } else {
                        info!("page model not found for removal");
                    }
                }
            }
            MessagesPageMsg::RefreshTopicTab {
                connection_id,
                topic_name,
            } => {
                info!(
                    "refresh topic tab[connection_id={}, topic_name={}]",
                    connection_id, topic_name
                );
                if let Some(conn) = self.connection.clone() {
                    if let Some(conn_id) = conn.id {
                        if conn_id == connection_id {
                            let topics = self.topics.guard();
                            for i in 0..topics.len() {
                                let tp = topics.get(i);
                                if let Some(tp) = tp {
                                    if let Some(topic) = tp.topic.clone() {
                                        if topic.name == topic_name {
                                            info!(
                                                "refresh topic tab: {}-{}",
                                                connection_id, topic_name
                                            );
                                            topics.send(i, MessagesTabMsg::RefreshTopic);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        self.update_view(widgets, sender);
    }
}

impl MessagesPageModel {
    fn get_tab_page_by_title(
        &self,
        widgets: &mut MessagesPageModelWidgets,
        title: String,
    ) -> Option<(usize, TabPage)> {
        let mut has_page: Option<(usize, TabPage)> = None;
        for i in 0..widgets.topics_viewer.n_pages() {
            let tab = widgets.topics_viewer.nth_page(i);
            if title == tab.title() {
                has_page = Some((i as usize, tab.clone()));
                break;
            }
        }
        has_page
    }
    fn get_model_by_tab_page(&mut self, page: TabPage) -> Option<KrustTopic> {
        info!("get_model_by_tab_page page title{}", page.title());
        let mut model: Option<KrustTopic> = None;
        let mut topics = self.topics.guard();
        for i in 0..topics.len() {
            let tp = topics.get_mut(i);
            if let Some(tp) = tp {
                let title = format!(
                    "[{}] {}",
                    tp.connection.clone().unwrap().name.clone(),
                    tp.topic.clone().unwrap().name.clone()
                );
                info!("PageClosed [{}][{}={}]", i, title, page.title());
                if title.eq(&page.title().to_string()) {
                    model = tp.topic.clone();
                    break;
                }
            }
        }
        model
    }
    fn get_tab_page_by_index(
        &self,
        widgets: &mut MessagesPageModelWidgets,
        idx: i32,
    ) -> Option<TabPage> {
        let tab = widgets.topics_viewer.nth_page(idx);
        Some(tab)
    }
}
