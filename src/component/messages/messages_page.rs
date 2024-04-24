#![allow(deprecated)]
// See: https://gitlab.gnome.org/GNOME/gtk/-/issues/5644
use crate::{
    backend::repository::{KrustConnection, KrustTopic},
    Repository,
};
use adw::TabPage;
use relm4::{factory::FactoryVecDeque, *};
use sourceview::prelude::*;
use sourceview5 as sourceview;

use super::messages_tab::{MessagesTabInit, MessagesTabModel};

pub struct MessagesPageModel {
    topic: Option<KrustTopic>,
    connection: Option<KrustConnection>,
    topics: FactoryVecDeque<MessagesTabModel>,
}

#[derive(Debug)]
pub enum MessagesPageMsg {
    Open(KrustConnection, KrustTopic),
    PageAdded(TabPage, i32),
}

#[relm4::component(pub)]
impl Component for MessagesPageModel {
    type Init = ();
    type Input = MessagesPageMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            append = &adw::TabBar {
                set_autohide: false,
                set_view: Some(&topics_viewer),
            },
            #[local_ref]
            topics_viewer -> adw::TabView {}
        }
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

        topics_viewer.connect_page_attached(move |_tab_view, page, n| {
            sender.input(MessagesPageMsg::PageAdded(page.clone(), n));
        });
        let widgets = view_output!();

        let model = MessagesPageModel {
            topic: None,
            connection: None,
            topics: topics,
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
                let conn_id = &connection.id.unwrap();
                let topic_name = &topic.name.clone();
                self.connection = Some(connection);
                let mut repo = Repository::new();
                let maybe_topic = repo.find_topic(*conn_id, topic_name);
                self.topic = maybe_topic.clone().or(Some(topic));
                let init = MessagesTabInit {
                    topic: self.topic.clone().unwrap(),
                    connection: self.connection.clone().unwrap(),
                };
                let _index = self.topics.guard().push_front(init);
            }
            MessagesPageMsg::PageAdded(page, index) => {
                let tab_model = self.topics.get(index.try_into().unwrap()).unwrap();
                let title = format!(
                    "[{}] {}",
                    tab_model.connection.clone().unwrap().name,
                    tab_model.topic.clone().unwrap().name
                );
                page.set_title(title.as_str());
                widgets.topics_viewer.set_selected_page(&page);
            }
        };

        self.update_view(widgets, sender);
    }
}
