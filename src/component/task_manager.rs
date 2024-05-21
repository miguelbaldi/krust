//! Alert dialog for displaying arbitrary errors.
//!
//! Inspired by [`relm4_components::alert`], but allows sending the dialog text as part of the
//! `Show` message, and supports displaying only a single button to dismiss.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gtk::glib::SignalHandlerId;
use gtk::{pango, prelude::*};

use relm4::binding::{Binding, BoolBinding, F64Binding, StringBinding};

use relm4::typed_view::list::{RelmListItem, TypedListView};
use relm4::{prelude::*, RelmObjectExt};
use relm4::{MessageBroker, Sender};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::*;
use uuid::Uuid;

pub static TASK_MANAGER_BROKER: MessageBroker<TaskManagerMsg> = MessageBroker::new();

// START: sidebar_list
#[derive(Debug)]
struct SidebarListItem {
    variant: TaskVariant,
    label: StringBinding,
    spin: BoolBinding,
    counter: Arc<Mutex<u8>>,
}

impl SidebarListItem {
    fn new(variant: TaskVariant) -> Self {
        Self {
            variant: variant,
            label: StringBinding::default(),
            spin: BoolBinding::new(true),
            counter: Arc::new(Mutex::new(1)),
        }
    }
}

struct Widgets {
    spinner: gtk::Spinner,
    name: gtk::Label,
}

impl Drop for Widgets {
    fn drop(&mut self) {
        dbg!(self.name.label());
    }
}

impl SidebarListItem {
    fn label(variant: &TaskVariant, counter: u8) -> String {
        match variant {
            TaskVariant::FetchMessages => {
                if counter > 1 {
                    format!("Fetching {} topics", &counter)
                } else {
                    format!("Fetching topic")
                }
            }
        }
    }
    fn label_done(variant: &TaskVariant) -> String {
        match variant {
            TaskVariant::FetchMessages => {
                format!("Fetching done!")
            }
        }
    }
}

impl RelmListItem for SidebarListItem {
    type Root = gtk::Box;
    type Widgets = Widgets;

    fn setup(_item: &gtk::ListItem) -> (gtk::Box, Widgets) {
        relm4::view! {
            task_box = gtk::Box {
                add_css_class: "task-manager",
                set_spacing: 9,
                #[name = "spinner"]
                gtk::Spinner {},

                #[name = "name"]
                gtk::Label {
                    set_ellipsize: pango::EllipsizeMode::End,
                },
            }
        }

        let widgets = Widgets { name, spinner };

        (task_box, widgets)
    }

    fn bind(&mut self, widgets: &mut Self::Widgets, _root: &mut Self::Root) {
        let Widgets { name, spinner } = widgets;
        name.add_write_only_binding(&self.label, "label");
        spinner.add_write_only_binding(&self.spin, "spinning");
    }
}
// END: sidebar_list
// START: tasks_list
#[derive(Debug)]
struct TaskListItem {
    value: Task,
    progress: F64Binding,
    sender: Sender<TaskManagerCommand>,
    cancel_handler_id: RefCell<Option<SignalHandlerId>>,
}

impl TaskListItem {
    fn new(value: Task, sender: Sender<TaskManagerCommand>) -> Self {
        Self {
            value,
            progress: F64Binding::new(0.0),
            sender: sender,
            cancel_handler_id: RefCell::new(None),
        }
    }
}

struct TaskWidgets {
    task_progress: gtk::Box,
    task_name: gtk::Label,
    progress_bar: gtk::ProgressBar,
    cancel_button: gtk::Button,
}

impl Drop for TaskWidgets {
    fn drop(&mut self) {
        dbg!(self
            .progress_bar
            .text()
            .map(|t| t.to_string())
            .unwrap_or_default());
    }
}

impl TaskListItem {
    fn task_label(&mut self) -> String {
        match self.value.variant {
            TaskVariant::FetchMessages => {
                format!("Fetching {}", &self.value.name.clone().unwrap_or_default())
            }
        }
    }
}

impl RelmListItem for TaskListItem {
    type Root = gtk::Box;
    type Widgets = TaskWidgets;

    fn setup(_item: &gtk::ListItem) -> (gtk::Box, TaskWidgets) {
        relm4::view! {
            task_box = gtk::Box {
                set_spacing: 9,
                set_width_request: 450,
                set_hexpand: true,
                set_orientation: gtk::Orientation::Horizontal,
                #[name = "task_progress"]
                gtk::Box {
                    set_hexpand: true,
                    set_orientation: gtk::Orientation::Vertical,
                    #[name = "task_name"]
                    gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_ellipsize: pango::EllipsizeMode::End,
                    },
                    #[name = "progress_bar"]
                    gtk::ProgressBar {
                        set_hexpand: true,
                        set_show_text: true,
                        set_ellipsize: pango::EllipsizeMode::End,
                    },
                },
                #[name = "cancel_button"]
                gtk::Button {
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_vexpand: false,
                    set_margin_start: 5,
                    add_css_class: "circular",
                    set_icon_name: "media-playback-stop-symbolic",
                    set_tooltip_text: Some("Tries to cancel task"),
                },
            }
        }
        let widgets = TaskWidgets {
            task_progress,
            task_name,
            progress_bar,
            cancel_button,
        };

        (task_box, widgets)
    }

    fn bind(&mut self, widgets: &mut Self::Widgets, _root: &mut Self::Root) {
        let TaskWidgets {
            task_progress,
            task_name,
            progress_bar,
            cancel_button,
        } = widgets;
        task_progress.set_tooltip_text(Some(&self.task_label()));
        task_name.set_label(&self.task_label());
        progress_bar.add_write_only_binding(&self.progress, "fraction");
        cancel_button.set_sensitive(self.value.token.is_some());
        if self.value.token.is_some() {
            let task = self.value.clone();
            let token = self.value.clone().token.unwrap();
            let sender = self.sender.clone();
            let signal_id = cancel_button.connect_clicked(move |_button| {
                sender.emit(TaskManagerCommand::CancelTask(task.clone()));
                token.cancel();
            });
            self.cancel_handler_id = RefCell::new(Some(signal_id));
        };
    }
    fn unbind(&mut self, widgets: &mut Self::Widgets, _root: &mut Self::Root) {
        if let Some(id) = self.cancel_handler_id.take() {
            widgets.cancel_button.disconnect(id);
        };
    }
}
// END: tasks_list

#[derive(Debug)]
pub struct TaskManagerModel {
    sidebar_list_wrapper: TypedListView<SidebarListItem, gtk::NoSelection>,
    tasks_list_wrapper: TypedListView<TaskListItem, gtk::NoSelection>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskVariant {
    FetchMessages,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub variant: TaskVariant,
    pub name: Option<String>,
    pub token: Option<CancellationToken>,
    _private: (),
}

impl Task {
    pub fn new(
        variant: TaskVariant,
        name: Option<String>,
        token: Option<CancellationToken>,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            id: id,
            variant: variant,
            name: name,
            token: token,
            _private: (),
        }
    }
}

#[derive(Debug)]
pub enum TaskManagerMsg {
    AddTask(Task),
    RemoveTask(Task),
    Progress(Task, f64),
}

#[derive(Debug)]
pub enum TaskManagerCommand {
    RemoveTask(Task),
    RemoveSidebarTask(u32),
    NeedsAttention,
    RemoveAttention,
    CancelTask(Task),
}

#[relm4::component(pub)]
impl Component for TaskManagerModel {
    type Widgets = TaskManagerWidgets;
    type Init = ();
    type Input = TaskManagerMsg;
    type Output = ();
    type CommandOutput = TaskManagerCommand;

    view! {
        adw::Bin {
            set_margin_all: 0,
            set_height_request: 54,
            set_hexpand: true,
            set_widget_name: "TaskManager",
            #[name(tasks_button)]
            gtk::MenuButton {
                set_tooltip_text: Some("Show running tasks"),
                set_direction: gtk::ArrowType::Right,
                add_css_class: "flat",
                #[wrap(Some)]
                set_popover: tasks_popover = &gtk::Popover {
                    set_position: gtk::PositionType::Right,
                    #[wrap(Some)]
                    set_child = &gtk::ScrolledWindow {
                        set_hscrollbar_policy: gtk::PolicyType::Never,
                        set_max_content_height: 270,
                        set_propagate_natural_height: true,
                        model.tasks_list_wrapper.view.borrow() -> &gtk::ListView {
                            set_margin_all: 6,
                            add_css_class: "tasks-list",
                            add_css_class: "rich-list",
                            set_show_separators: true,
                        },
                    },
                },
                #[wrap(Some)]
                set_child = model.sidebar_list_wrapper.view.borrow() -> &gtk::ListView {
                    set_valign: gtk::Align::Center,
                },
            }
        }
    }

    fn init(_: (), root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        // Initialize the ListView wrapper
        let sidebar_list_view_wrapper: TypedListView<SidebarListItem, gtk::NoSelection> =
            TypedListView::default();
        let task_list_view_wrapper: TypedListView<TaskListItem, gtk::NoSelection> =
            TypedListView::default();
        let model = TaskManagerModel {
            sidebar_list_wrapper: sidebar_list_view_wrapper,
            tasks_list_wrapper: task_list_view_wrapper,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        input: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match input {
            TaskManagerMsg::AddTask(task) => {
                widgets.tasks_button.set_sensitive(true);
                let maybe_index = self
                    .sidebar_list_wrapper
                    .find(|t| t.variant == task.variant);
                if let Some(idx) = maybe_index {
                    let found = self.sidebar_list_wrapper.get(idx).unwrap();
                    let item = &mut found.borrow_mut();
                    let counter = item.counter.clone();
                    let mut counter = counter.lock().unwrap();
                    *counter += 1;
                    let label = &mut item.label;
                    let mut guard = label.guard();
                    *guard = SidebarListItem::label(&task.variant, *counter);
                    let spinner = &mut item.spin;
                    let mut guard = spinner.guard();
                    *guard = true;
                } else {
                    let item = SidebarListItem::new(task.variant.clone());
                    let mut label = item.label.guard();
                    let mut spinner = item.spin.guard();
                    *label = SidebarListItem::label(&task.variant, *item.counter.lock().unwrap());
                    *spinner = true;
                    self.sidebar_list_wrapper.append(item);
                }
                let item_sender = sender.command_sender().clone();
                self.tasks_list_wrapper
                    .append(TaskListItem::new(task, item_sender));
                sender
                    .command_sender()
                    .emit(TaskManagerCommand::NeedsAttention);
            }
            TaskManagerMsg::Progress(task, step) => {
                let maybe_index = self.tasks_list_wrapper.find(|t| t.value.id.eq(&task.id));
                if let Some(idx) = maybe_index {
                    let found = self.tasks_list_wrapper.get(idx).unwrap();
                    let item = &mut found.borrow_mut();
                    let progress = &mut item.progress;
                    let mut guard = progress.guard();
                    *guard += step;
                    info!(
                        "task_manager::progress::received::{}={}",
                        item.value.id, *guard
                    );
                    if *guard >= 1.0 {
                        sender.input(TaskManagerMsg::RemoveTask(task.clone()));
                    }
                }
            }
            TaskManagerMsg::RemoveTask(task) => {
                let maybe_index = self
                    .sidebar_list_wrapper
                    .find(|t| t.variant == task.variant);
                if let Some(idx) = maybe_index {
                    let found = self.sidebar_list_wrapper.get(idx).unwrap();
                    let item = &mut found.borrow_mut();
                    let counter = item.counter.clone();
                    let counter = counter.lock().unwrap();
                    if *counter <= 1 {
                        let label = &mut item.label;
                        let mut guard = label.guard();
                        *guard = SidebarListItem::label_done(&item.variant);
                        let spinner = &mut item.spin;
                        let mut guard = spinner.guard();
                        *guard = false;
                    }
                }
                sender.oneshot_command(async move {
                    sleep(Duration::from_secs(5)).await;
                    info!("removing task with index: {}", task.id);
                    TaskManagerCommand::RemoveTask(task.clone())
                });
            }
        }
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            TaskManagerCommand::RemoveTask(task) => {
                let maybe_index = self
                    .sidebar_list_wrapper
                    .find(|t| t.variant == task.variant);
                if let Some(idx) = maybe_index {
                    let found = self.sidebar_list_wrapper.get(idx).unwrap();
                    let item = &mut found.borrow_mut();
                    let counter = item.counter.clone();
                    let mut counter = counter.lock().unwrap();
                    if *counter <= 1 {
                        sender
                            .command_sender()
                            .emit(TaskManagerCommand::RemoveSidebarTask(idx));
                    } else {
                        *counter -= 1;
                        let label = &mut item.label;
                        let mut guard = label.guard();
                        *guard = SidebarListItem::label(&task.variant, *counter);
                    }
                }
                let maybe_index = self.tasks_list_wrapper.find(|t| t.value.id.eq(&task.id));
                if let Some(idx) = maybe_index {
                    self.tasks_list_wrapper.remove(idx);
                }
            }
            TaskManagerCommand::RemoveSidebarTask(idx) => {
                self.sidebar_list_wrapper.remove(idx);
                widgets.tasks_popover.popdown();
                widgets.tasks_button.set_sensitive(false);
            }
            TaskManagerCommand::NeedsAttention => {
                root.add_css_class("needs-attention");
                gtk::glib::timeout_add_once(Duration::from_secs(2), move || {
                    sender
                        .command_sender()
                        .emit(TaskManagerCommand::RemoveAttention);
                });
            }
            TaskManagerCommand::RemoveAttention => {
                root.remove_css_class("needs-attention");
                root.queue_draw();
            }
            TaskManagerCommand::CancelTask(task) => {
                info!("cancel task {:?}", task);
                sender
                    .command_sender()
                    .emit(TaskManagerCommand::RemoveTask(task));
            }
        }
    }
}
