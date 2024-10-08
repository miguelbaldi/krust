// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use crate::{
    backend::repository::{KrustConnection, KrustTopic},
    component::{
        colorize_widget_by_connection, get_tab_by_title,
        topics::topics_tab::{TopicsTabInit, TopicsTabOutput},
    },
};
use adw::{prelude::*, TabPage};
use relm4::{actions::RelmAction, factory::FactoryVecDeque, *};
use tracing::*;

use super::topics_tab::TopicsTabModel;

relm4::new_action_group!(pub(super) TopicListActionGroup, "topic-list");
relm4::new_stateless_action!(pub(super) FavouriteAction, TopicListActionGroup, "toggle-favourite");

relm4::new_action_group!(pub(super) ConnectionTabActionGroup, "connection-tab");
relm4::new_stateless_action!(pub(super) PinTabAction, ConnectionTabActionGroup, "toggle-pin");
relm4::new_stateless_action!(pub(super) CloseTabAction, ConnectionTabActionGroup, "close");

pub struct TopicsPageModel {
    pub current: Option<KrustConnection>,
    pub topics: FactoryVecDeque<TopicsTabModel>,
    pub is_loading: bool,
    pub search_text: String,
}

#[derive(Debug)]
pub enum TopicsPageMsg {
    Open(KrustConnection),
    PageAdded(TabPage, i32),
    MenuPageClosed,
    MenuPagePin,
}

#[derive(Debug)]
pub enum TopicsPageOutput {
    OpenMessagesPage(KrustConnection, KrustTopic),
    HandleError(KrustConnection, bool),
}

#[relm4::component(pub)]
impl Component for TopicsPageModel {
    type Init = Option<KrustConnection>;
    type Input = TopicsPageMsg;
    type Output = TopicsPageOutput;
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
        current: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let topics = FactoryVecDeque::builder()
            .launch(adw::TabView::default())
            .forward(sender.output_sender(), |msg| match msg {
                TopicsTabOutput::OpenMessagesPage(conn, topic) => {
                    TopicsPageOutput::OpenMessagesPage(conn, topic)
                }
                TopicsTabOutput::HandleError(conn, disconnect) => {
                    info!("[topics_page] Handle error: {} - {}", conn.name, disconnect);
                    TopicsPageOutput::HandleError(conn, disconnect)
                }
            });

        let topics_viewer: &adw::TabView = topics.widget();
        topics_viewer.connect_setup_menu(|view, page| {
            if let Some(page) = page {
                view.set_selected_page(page);
            }
        });
        let tabs_sender = sender.clone();
        topics_viewer.connect_page_attached(move |_tab_view, page, n| {
            tabs_sender.input(TopicsPageMsg::PageAdded(page.clone(), n));
        });

        let widgets = view_output!();

        let mut topics_tabs_actions =
            relm4::actions::RelmActionGroup::<ConnectionTabActionGroup>::new();
        let tabs_sender = sender.input_sender().clone();
        let close_tab_action = RelmAction::<CloseTabAction>::new_stateless(move |_| {
            tabs_sender.send(TopicsPageMsg::MenuPageClosed).unwrap();
        });
        let tabs_sender = sender.input_sender().clone();
        let pin_tab_action = RelmAction::<PinTabAction>::new_stateless(move |_| {
            tabs_sender.send(TopicsPageMsg::MenuPagePin).unwrap();
        });
        topics_tabs_actions.add_action(close_tab_action);
        topics_tabs_actions.add_action(pin_tab_action);
        topics_tabs_actions.register_for_widget(&widgets.topics_tabs);

        let model = TopicsPageModel {
            current,
            topics,
            is_loading: false,
            search_text: String::default(),
        };
        ComponentParts { model, widgets }
    }
    fn post_view(&self, widgets: &mut Self::Widgets) {

        //widgets.topics_tabs.remove_css_class(&css_class);
        //widgets.topics_tabs.add_css_class(&css_class);
    }
    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: TopicsPageMsg,
        sender: ComponentSender<Self>,
        _: &Self::Root,
    ) {
        match msg {
            TopicsPageMsg::Open(connection) => {
                let mut has_page: Option<(usize, TabPage)> = None;
                for i in 0..widgets.topics_viewer.n_pages() {
                    let tab = widgets.topics_viewer.nth_page(i);
                    let title = connection.name.clone();
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
                        self.current = Some(connection);
                        let init = TopicsTabInit {
                            connection: self.current.clone().unwrap(),
                        };
                        let _index = self.topics.guard().push_front(init);
                    }
                }
            }
            TopicsPageMsg::PageAdded(page, index) => {
                let tab_model = self.topics.get(index.try_into().unwrap()).unwrap();
                let conn = tab_model.current.clone().unwrap();
                let title = tab_model.current.clone().unwrap().name;
                page.set_title(title.as_str());
                page.set_live_thumbnail(true);
                let maybe_tab = get_tab_by_title(&widgets.topics_tabs, title);

                if let Some(tab) = maybe_tab {
                    colorize_widget_by_connection(&conn, tab);
                }
                widgets.topics_viewer.set_selected_page(&page);
            }
            TopicsPageMsg::MenuPagePin => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    let pinned = !page.is_pinned();
                    widgets.topics_viewer.set_page_pinned(&page, pinned);
                }
            }
            TopicsPageMsg::MenuPageClosed => {
                let page = widgets.topics_viewer.selected_page();
                if let Some(page) = page {
                    info!("closing messages page with name {}", page.title());
                    let mut idx: Option<usize> = None;
                    let mut topics = self.topics.guard();
                    for i in 0..topics.len() {
                        let tp = topics.get_mut(i);
                        if let Some(tp) = tp {
                            let title = tp.current.clone().unwrap().name.clone();
                            info!("PageClosed [{}][{}={}]", i, title, page.title());
                            if title.eq(&page.title().to_string()) {
                                idx = Some(i);
                                break;
                            }
                        }
                    }
                    if let Some(idx) = idx {
                        let result = topics.remove(idx);
                        info!(
                            "page model with index {} and name {:?} removed",
                            idx,
                            result.is_some()
                        );
                    } else {
                        info!("page model not found for removal");
                    }
                }
            }
        };

        self.update_view(widgets, sender);
    }
}
