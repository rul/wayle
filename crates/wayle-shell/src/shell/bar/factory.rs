//! Bar item factory for creating modules and groups.

use std::{cell::Cell, rc::Rc};

use gtk::prelude::*;
use relm4::prelude::*;
use wayle_config::schemas::bar::BarItem;
use wayle_widgets::prelude::BarSettings;

use crate::shell::{
    bar::{
        dropdowns::DropdownRegistry,
        modules::{ModuleInstance, create_module},
    },
    services::ShellServices,
};

pub(crate) struct BarItemFactoryInit {
    pub(crate) item: BarItem,
    pub(crate) settings: BarSettings,
    pub(crate) services: ShellServices,
    pub(crate) dropdowns: Rc<DropdownRegistry>,
}

pub(crate) struct BarItemFactory {
    item: BarItem,
    settings: BarSettings,
    #[allow(dead_code)]
    services: ShellServices,
    #[allow(dead_code)]
    dropdowns: Rc<DropdownRegistry>,
    modules: Vec<ModuleInstance>,
}

#[relm4::factory(pub(crate))]
impl FactoryComponent for BarItemFactory {
    type Init = BarItemFactoryInit;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Box {
            add_css_class: "bar-item",
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let modules = match &init.item {
            BarItem::Module(module) => {
                create_module(module, &init.settings, &init.services, &init.dropdowns)
                    .into_iter()
                    .collect()
            }
            BarItem::Group(group) => group
                .modules
                .iter()
                .filter_map(|module| {
                    create_module(module, &init.settings, &init.services, &init.dropdowns)
                })
                .collect(),
        };

        Self {
            item: init.item,
            settings: init.settings,
            services: init.services,
            dropdowns: init.dropdowns,
            modules,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        let orientation = if self.settings.is_vertical.get() {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        };
        root.set_orientation(orientation);

        if let BarItem::Group(group) = &self.item {
            root.set_widget_name(&group.name);
            root.add_css_class("bar-group");
        }

        let drawer =
            matches!(&self.item, BarItem::Group(group) if group.drawer) && self.modules.len() > 1;
        let expanded = Rc::new(Cell::new(false));
        let mut members = Vec::new();
        for (index, instance) in self.modules.iter().enumerate() {
            let widget = instance.controller.widget();
            widget.add_css_class("module");
            if let Some(class) = &instance.class {
                widget.add_css_class(class);
            }
            if drawer && index > 0 {
                let contents = gtk::Box::new(orientation, 0);
                contents.add_css_class("drawer-contents");
                contents.append(widget);
                let revealer = gtk::Revealer::builder()
                    .transition_duration(250)
                    .transition_type(if self.settings.is_vertical.get() {
                        gtk::RevealerTransitionType::SlideDown
                    } else {
                        gtk::RevealerTransitionType::SlideRight
                    })
                    .child(&contents)
                    .reveal_child(widget.has_css_class("drawer-active"))
                    .build();
                root.append(&revealer);
                let reveal = revealer.downgrade();
                let expanded = expanded.clone();
                widget.connect_notify_local(Some("css-classes"), move |widget, _| {
                    if let Some(reveal) = reveal.upgrade() {
                        reveal.set_reveal_child(drawer_member_visible(
                            expanded.get(),
                            widget.has_css_class("drawer-active"),
                        ));
                    }
                });
                members.push((revealer, widget.clone()));
            } else {
                root.append(widget);
            }

            let container = root.downgrade();
            widget.connect_notify_local(Some("visible"), move |_, _| {
                if let Some(container) = container.upgrade() {
                    sync_container_visibility(&container);
                }
            });
        }

        if drawer {
            root.add_css_class("bar-drawer");
            let members = Rc::new(members);
            let set_expanded: Rc<dyn Fn(bool)> = Rc::new(move |open| {
                expanded.set(open);
                for (reveal, widget) in members.iter() {
                    reveal.set_reveal_child(drawer_member_visible(
                        open,
                        widget.has_css_class("drawer-active"),
                    ));
                }
            });
            let motion = gtk::EventControllerMotion::new();
            let focus = gtk::EventControllerFocus::new();
            let update = set_expanded.clone();
            motion.connect_enter(move |_, _, _| update(true));
            let update = set_expanded.clone();
            let focus_state = focus.downgrade();
            motion.connect_leave(move |_| {
                update(
                    focus_state
                        .upgrade()
                        .is_some_and(|focus| focus.contains_focus()),
                );
            });
            let update = set_expanded.clone();
            focus.connect_enter(move |_| update(true));
            let motion_state = motion.downgrade();
            focus.connect_leave(move |_| {
                set_expanded(
                    motion_state
                        .upgrade()
                        .is_some_and(|motion| motion.contains_pointer()),
                );
            });
            root.add_controller(motion);
            root.add_controller(focus);
        }
        sync_container_visibility(&root);

        widgets
    }
}

impl BarItemFactory {
    pub(crate) fn matches(&self, item: &BarItem) -> bool {
        self.item == *item
    }
}

fn sync_container_visibility(container: &gtk::Box) {
    let has_visible_child = container
        .observe_children()
        .into_iter()
        .filter_map(|obj| obj.ok()?.downcast::<gtk::Widget>().ok())
        .any(|widget| has_visible_module(&widget));

    container.set_visible(has_visible_child);
}

fn has_visible_module(widget: &gtk::Widget) -> bool {
    if !widget.get_visible() {
        return false;
    }
    if widget.has_css_class("module") {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if has_visible_module(&current) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

// The first module is the handle. Other members stay revealed while active.
fn drawer_member_visible(expanded: bool, active: bool) -> bool {
    expanded || active
}

#[cfg(test)]
mod tests {
    use super::drawer_member_visible;

    #[test]
    fn collapsed_drawer_keeps_only_active_members() {
        assert!(!drawer_member_visible(false, false));
        assert!(drawer_member_visible(false, true));
    }

    #[test]
    fn expanded_drawer_shows_all_members() {
        assert!(drawer_member_visible(true, false));
        assert!(drawer_member_visible(true, true));
    }
}
