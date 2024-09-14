#![allow(deprecated)]

// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use crate::{
    backend::repository::{KrustConnection, KrustTopic},
    Repository,
};
use adw::TabPage;
use relm4::{actions::RelmAction, factory::FactoryVecDeque, *};
use sourceview::prelude::*;
use sourceview5 as sourceview;
use tracing::info;

use super::messages_tab::{MessagesTabInit, MessagesTabModel};

relm4::new_action_group!(pub(super) TopicTabActionGroup, "topic-tab");
relm4::new_stateless_action!(pub(super) PinTabAction, TopicTabActionGroup, "toggle-pin");
relm4::new_stateless_action!(pub(super) CloseTabAction, TopicTabActionGroup, "close");

pub struct MessagesPageModel {
    topic: Option<KrustTopic>,
    connection: Option<KrustConnection>,
    topics: FactoryVecDeque<MessagesTabModel>,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
    Open(Box<KrustConnection>, Box<KrustTopic>),
    PageAdded(TabPage, i32),
    MenuPageClosed,
    MenuPagePin,
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
        topics_viewer.connect_page_attached(move |_tab_view, page, n| {
            tabs_sender.input(MessagesPageMsg::PageAdded(page.clone(), n));
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
        topics_tabs_actions.add_action(close_tab_action);
        topics_tabs_actions.add_action(pin_tab_action);
        topics_tabs_actions.register_for_widget(&widgets.topics_tabs);

        let model = MessagesPageModel {
            topic: None,
            connection: None,
            topics,
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
                let mut has_page: Option<(usize, TabPage)> = None;
                for i in 0..widgets.topics_viewer.n_pages() {
                    let tab = widgets.topics_viewer.nth_page(i);
                    let title = format!("[{}] {}", connection.name, topic.name);
                    if title == tab.title() {
                        has_page = Some((i as usize, tab.clone()));
                        break;
                    }
                }
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
            MessagesPageMsg::PageAdded(page, index) => {
                let tab_model = self.topics.get(index.try_into().unwrap()).unwrap();
                let title = format!(
                    "[{}] {}",
                    tab_model.connection.clone().unwrap().name,
                    tab_model.topic.clone().unwrap().name
                );
                page.set_title(title.as_str());
                page.set_live_thumbnail(true);
                widgets.topics_viewer.set_selected_page(&page);
            }
            MessagesPageMsg::MenuPagePin => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    let pinned = !page.is_pinned();
                    widgets.topics_viewer.set_page_pinned(&page, pinned);
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
        };

        self.update_view(widgets, sender);
    }
}
