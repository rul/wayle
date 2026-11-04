use std::sync::Arc;

use relm4::ComponentSender;
use tokio_util::sync::CancellationToken;
use wayle_config::ConfigService;
use wayle_network::NetworkService;
use wayle_widgets::{watch, watch_cancellable};

use super::{NetworkDropdown, messages::NetworkDropdownCmd};

pub(super) fn spawn(
    sender: &ComponentSender<NetworkDropdown>,
    config: &Arc<ConfigService>,
    network: &Arc<NetworkService>,
) {
    let scale = config.config().styling.scale.clone();
    watch!(sender, [scale.watch()], |out| {
        let _ = out.send(NetworkDropdownCmd::ScaleChanged(scale.get().value()));
    });

    let wifi = network.wifi.clone();
    let adapters = network.wifi_adapters.clone();
    watch!(sender, [wifi.watch(), adapters.watch()], |out| {
        let _ = out.send(NetworkDropdownCmd::WifiDeviceChanged);
    });
}

pub(super) fn spawn_wifi_watchers(
    sender: &ComponentSender<NetworkDropdown>,
    network: &Arc<NetworkService>,
    token: CancellationToken,
) {
    let Some(wifi) = network
        .wifi
        .get()
        .or_else(|| network.wifi_adapters.get().first().cloned())
    else {
        return;
    };

    let enabled = wifi.enabled.clone();
    watch_cancellable!(sender, token, [enabled.watch()], |out| {
        let _ = out.send(NetworkDropdownCmd::WifiEnabledChanged(enabled.get()));
    });
}
