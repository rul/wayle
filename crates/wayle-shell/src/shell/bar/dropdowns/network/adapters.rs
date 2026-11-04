//! Per-interface Wi-Fi controls. Disabling one adapter does not change the global radio.
use std::sync::Arc;

use gtk::prelude::*;
use relm4::prelude::*;
use wayle_network::NetworkService;
use zbus::zvariant::OwnedObjectPath;

use crate::i18n::t;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AdapterState {
    path: OwnedObjectPath,
    interface: String,
    driver: String,
    managed: bool,
    state: u32,
    radio_enabled: bool,
}

fn snapshot(network: &NetworkService) -> Vec<AdapterState> {
    network
        .wifi_adapters
        .get()
        .iter()
        .map(|adapter| {
            let device = &adapter.device.core;
            AdapterState {
                path: device.object_path.clone(),
                interface: device.interface.get(),
                driver: device.driver.get(),
                managed: device.managed.get(),
                state: device.state.get() as u32,
                radio_enabled: adapter.enabled.get(),
            }
        })
        .collect()
}

pub(super) struct WifiAdapters {
    network: Arc<NetworkService>,
    adapters: Vec<AdapterState>,
    rows: gtk::Box,
    pending: Option<(OwnedObjectPath, bool)>,
    error: Option<String>,
}

#[derive(Debug)]
pub(super) enum AdapterMsg {
    Toggle(OwnedObjectPath, bool),
}

#[derive(Debug)]
pub(super) enum AdapterCmd {
    Snapshot(Vec<AdapterState>),
    Finished(Result<(), String>),
}

#[relm4::component(pub(super))]
impl Component for WifiAdapters {
    type Init = Arc<NetworkService>;
    type Input = AdapterMsg;
    type Output = ();
    type CommandOutput = AdapterCmd;

    view! {
        gtk::Box {
            add_css_class: "wifi-adapters",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,
            #[watch]
            set_visible: !model.adapters.is_empty(),
            gtk::Label {
                add_css_class: "heading",
                set_halign: gtk::Align::Start,
                set_label: &t!("dropdown-network-adapters"),
            },
            #[local_ref]
            rows -> gtk::Box {},
            gtk::Label {
                add_css_class: "error",
                set_wrap: true,
                set_xalign: 0.0,
                #[watch]
                set_visible: model.error.is_some(),
                #[watch]
                set_label: model.error.as_deref().unwrap_or(""),
            },
        }
    }

    fn init(
        network: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Self {
            adapters: snapshot(&network),
            network: network.clone(),
            rows: gtk::Box::new(gtk::Orientation::Vertical, 6),
            pending: None,
            error: None,
        };
        model.render_rows(&sender);
        sender.command(move |out, shutdown| async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            let shutdown = shutdown.wait();
            tokio::pin!(shutdown);
            loop {
                tokio::select! {
                    () = &mut shutdown => break,
                    _ = interval.tick() => { let _ = out.send(AdapterCmd::Snapshot(snapshot(&network))); }
                }
            }
        });
        let rows = &model.rows;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        let AdapterMsg::Toggle(path, enabled) = msg;
        if self.pending.is_some() {
            return;
        }
        let Some(adapter) = self
            .network
            .wifi_adapters
            .get()
            .into_iter()
            .find(|a| a.device.core.object_path == path)
        else {
            return;
        };
        self.error = None;
        self.pending = Some((path, enabled));
        self.render_rows(&sender);
        sender.oneshot_command(async move {
            let result = async {
                adapter.device.core.set_managed(enabled).await?;
                if enabled {
                    adapter.device.core.set_autoconnect(true).await?;
                }
                Ok::<(), wayle_network::Error>(())
            }
            .await
            .map_err(|err| format!("{}: {err}", t!("dropdown-network-adapter-error")));
            AdapterCmd::Finished(result)
        });
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            AdapterCmd::Snapshot(adapters) => {
                if self.adapters == adapters {
                    return;
                }
                self.adapters = adapters;
                if self.pending.is_some() {
                    return;
                }
            }
            AdapterCmd::Finished(result) => {
                let pending = self.pending.take();
                self.adapters = snapshot(&self.network);
                if result.is_ok() {
                    if let Some((path, managed)) = pending {
                        if let Some(adapter) = self.adapters.iter_mut().find(|a| a.path == path) {
                            adapter.managed = managed;
                        }
                    }
                }
                self.error = result.err();
            }
        }
        self.render_rows(&sender);
    }
}

impl WifiAdapters {
    fn render_rows(&mut self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        for adapter in &self.adapters {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
            labels.set_hexpand(true);
            let label = gtk::Label::new(Some(&adapter.interface));
            label.set_xalign(0.0);
            label.set_tooltip_text(Some(&adapter.driver));
            labels.append(&label);
            let status = if !adapter.managed {
                t!("dropdown-network-adapter-disabled")
            } else if !adapter.radio_enabled {
                t!("dropdown-network-adapter-radio-off")
            } else if adapter.state < 30 {
                t!("dropdown-network-adapter-unavailable")
            } else if adapter.state == 100 {
                t!("dropdown-network-connected")
            } else {
                t!("dropdown-network-adapter-enabled")
            };
            let detail = gtk::Label::new(Some(&status));
            detail.set_xalign(0.0);
            detail.add_css_class("dim-label");
            labels.append(&detail);
            row.append(&labels);
            let requested = self
                .pending
                .as_ref()
                .filter(|(path, _)| path == &adapter.path);
            let switch = gtk::Switch::builder()
                .active(requested.map_or(adapter.managed, |(_, enabled)| *enabled))
                .valign(gtk::Align::Center)
                .sensitive(self.pending.is_none())
                .build();
            switch.set_tooltip_text(Some(&adapter.interface));
            switch.set_cursor_from_name(Some("pointer"));
            let path = adapter.path.clone();
            let sender = sender.input_sender().clone();
            switch.connect_state_set(move |_, enabled| {
                let _ = sender.send(AdapterMsg::Toggle(path.clone(), enabled));
                gtk::glib::Propagation::Stop
            });
            row.append(&switch);
            self.rows.append(&row);
        }
    }
}
