//! Window banner for showing messages.

use std::time::Duration;

use relm4::prelude::*;
use relm4::MessageBroker;

pub static BANNER_BROKER: MessageBroker<AppBannerMsg> = MessageBroker::new();

#[derive(Debug)]
pub struct AppBannerModel {}

#[derive(Debug)]
pub enum AppBannerMsg {
    Show(String),
    Hide,
}
#[derive(Debug)]
pub enum AppBannerCommand {
    LateHide,
}

#[relm4::component(pub)]
impl Component for AppBannerModel {
    type Widgets = AppBannerWidgets;
    type Init = ();
    type Input = AppBannerMsg;
    type Output = ();
    type CommandOutput = AppBannerCommand;

    view! {
        #[name(app_banner)]
        adw::Banner {}
    }

    fn init(_: (), root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = AppBannerModel {};

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        input: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ){
        match input {
            AppBannerMsg::Show(text) => {
                widgets.app_banner.set_title(text.as_str());
                widgets.app_banner.set_revealed(true);
            }
            AppBannerMsg::Hide => {
                gtk::glib::timeout_add_once(Duration::from_secs(2), move ||{
                    sender.command_sender().emit(AppBannerCommand::LateHide);
                });
            }
        }
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppBannerCommand::LateHide => {
                widgets.app_banner.set_title("");
                widgets.app_banner.set_revealed(false);
            }
        }
    }
}
