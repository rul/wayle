use std::sync::Arc;

use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wayle_core::Property;
use wayle_traits::{Reactive, ServiceMonitoring};
use zbus::Connection;

use super::{
    core::settings::Settings,
    discovery::NetworkServiceDiscovery,
    error::Error,
    proxy::manager::NetworkManagerProxy,
    service::NetworkService,
    types::connectivity::ConnectionType,
    wifi::{LiveWifiParams, Wifi},
    wired::{LiveWiredParams, Wired},
};

impl ServiceMonitoring for NetworkService {
    type Error = Error;

    async fn start_monitoring(&self) -> Result<(), Self::Error> {
        spawn_primary_monitoring(
            self.zbus_connection.clone(),
            self.primary.clone(),
            self.cancellation_token.child_token(),
        )
        .await?;

        spawn_device_monitoring(
            self.zbus_connection.clone(),
            self.wifi.clone(),
            self.wifi_adapters.clone(),
            self.wired.clone(),
            self.settings.clone(),
            self.cancellation_token.child_token(),
        )
        .await
    }
}

async fn spawn_primary_monitoring(
    connection: Connection,
    primary: Property<ConnectionType>,
    cancellation_token: CancellationToken,
) -> Result<(), Error> {
    let nm_proxy = NetworkManagerProxy::new(&connection)
        .await
        .map_err(Error::DbusError)?;

    let initial_type = nm_proxy.primary_connection_type().await?;
    update_primary_connection(&initial_type, &primary);

    let mut type_changed = nm_proxy.receive_primary_connection_type_changed().await;

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    debug!("NetworkMonitoring primary monitoring cancelled");
                    return;
                }
                Some(change) = type_changed.next() => {
                    if let Ok(nm_type) = change.get().await {
                        debug!(nm_type = %nm_type, "Primary connection type changed");
                        update_primary_connection(&nm_type, &primary);
                    }
                }
            }
        }
    });

    Ok(())
}

async fn spawn_device_monitoring(
    connection: Connection,
    wifi: Property<Option<Arc<Wifi>>>,
    adapters: Property<Vec<Arc<Wifi>>>,
    wired: Property<Option<Arc<Wired>>>,
    settings: Arc<Settings>,
    cancellation_token: CancellationToken,
) -> Result<(), Error> {
    let nm_proxy = NetworkManagerProxy::new(&connection)
        .await
        .map_err(Error::DbusError)?;

    let mut device_added = nm_proxy.receive_device_added().await?;
    let mut device_removed = nm_proxy.receive_device_removed().await?;

    tokio::spawn(async move {
        let mut refresh = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tokio::select! {
                _ = refresh.tick() => {
                    refresh_wifi_adapters(&connection, &wifi, &adapters, &settings, &cancellation_token).await;
                }
                _ = cancellation_token.cancelled() => {
                    debug!("NetworkMonitoring device monitoring cancelled");
                    return;
                }
                Some(signal) = device_added.next() => {
                    let Ok(args) = signal.args() else { continue };
                    debug!(path = %args.device_path, "Network device added");

                    refresh_wifi_adapters(&connection, &wifi, &adapters, &settings, &cancellation_token).await;
                    try_initialize_wired(&connection, &wired, &cancellation_token).await;
                }
                Some(signal) = device_removed.next() => {
                    let Ok(args) = signal.args() else { continue };
                    debug!(path = %args.device_path, "Network device removed");

                    refresh_wifi_adapters(&connection, &wifi, &adapters, &settings, &cancellation_token).await;
                    handle_wired_removed(&args.device_path, &wired);
                }
            }
        }
    });

    Ok(())
}

pub(crate) async fn refresh_wifi_adapters(
    connection: &Connection,
    wifi: &Property<Option<Arc<Wifi>>>,
    adapters: &Property<Vec<Arc<Wifi>>>,
    settings: &Arc<Settings>,
    cancellation_token: &CancellationToken,
) {
    let paths = match NetworkServiceDiscovery::wifi_device_paths(connection).await {
        Ok(paths) => paths,
        Err(err) => {
            warn!(error = %err, "Cannot enumerate Wi-Fi adapters");
            return;
        }
    };
    let previous = adapters.get();
    let mut current = Vec::new();
    for path in paths {
        if let Some(adapter) = previous.iter().find(|a| a.device.core.object_path == path) {
            current.push(adapter.clone());
        } else {
            match Wifi::get_live(LiveWifiParams {
                connection,
                device_path: path,
                cancellation_token,
                settings: settings.clone(),
            })
            .await
            {
                Ok(adapter) => current.push(adapter),
                Err(err) => warn!(error = %err, "Cannot initialize Wi-Fi adapter"),
            }
        }
    }
    current.sort_by_key(|a| a.device.core.interface.get());
    let old = wifi.get();
    let candidates: Vec<_> = current
        .iter()
        .map(|a| {
            (
                a.device.core.managed.get(),
                a.device.core.state.get() as u32,
                old.as_ref()
                    .is_some_and(|old| old.device.core.object_path == a.device.core.object_path),
            )
        })
        .collect();
    let selected = preferred_adapter_index(&candidates).map(|index| current[index].clone());
    adapters.set(current);
    wifi.set(selected);
}

async fn try_initialize_wired(
    connection: &Connection,
    wired: &Property<Option<Arc<Wired>>>,
    cancellation_token: &CancellationToken,
) {
    if wired.get().is_some() {
        return;
    }

    let Some(path) = NetworkServiceDiscovery::wired_device_path(connection)
        .await
        .ok()
        .flatten()
    else {
        return;
    };

    match Wired::get_live(LiveWiredParams {
        connection,
        device_path: path.clone(),
        cancellation_token,
    })
    .await
    {
        Ok(new_wired) => {
            debug!(path = %path, "Wired device initialized");
            wired.set(Some(new_wired));
        }
        Err(err) => {
            warn!(error = %err, path = %path, "Failed to initialize wired device");
        }
    }
}

fn handle_wired_removed(device_path: &str, wired: &Property<Option<Arc<Wired>>>) {
    let Some(current) = wired.get() else { return };

    if current.device.core.object_path.as_str() == device_path {
        debug!(path = %device_path, "Wired device removed");
        wired.set(None);
    }
}

fn update_primary_connection(nm_type: &str, primary: &Property<ConnectionType>) {
    let connection_type = ConnectionType::from_nm_type(nm_type);
    debug!(?connection_type, "Primary connection type resolved");
    primary.set(connection_type);
}

// Prefer connected, then connecting adapters, keeping the current adapter on ties.
fn preferred_adapter_index(candidates: &[(bool, u32, bool)]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, (managed, _, _))| *managed)
        .max_by_key(|(_, (_, state, selected))| {
            let rank = if *state == 100 {
                3
            } else if (40..100).contains(state) {
                2
            } else {
                1
            };
            (rank, *selected)
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod adapter_tests {
    use super::preferred_adapter_index;

    #[test]
    fn disabling_internal_adapter_moves_to_external_adapter() {
        assert_eq!(
            preferred_adapter_index(&[(false, 100, true), (true, 30, false)]),
            Some(1)
        );
    }
    #[test]
    fn connected_external_adapter_is_preferred() {
        assert_eq!(
            preferred_adapter_index(&[(true, 30, true), (true, 100, false)]),
            Some(1)
        );
    }
    #[test]
    fn stable_selection_when_both_adapters_are_connected() {
        assert_eq!(
            preferred_adapter_index(&[(true, 100, true), (true, 100, false)]),
            Some(0)
        );
    }
    #[test]
    fn unplugging_external_adapter_falls_back_to_internal() {
        assert_eq!(preferred_adapter_index(&[(true, 30, false)]), Some(0));
    }
    #[test]
    fn all_disabled_and_no_devices_have_no_selected_adapter() {
        assert_eq!(
            preferred_adapter_index(&[(false, 100, true), (false, 30, false)]),
            None
        );
        assert_eq!(preferred_adapter_index(&[]), None);
    }
}
