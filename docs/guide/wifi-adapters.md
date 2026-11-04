# Multiple Wi-Fi adapters

Open the network dropdown and use the switches under **Wi-Fi adapters**.
Each row identifies an interface, with its driver available as a tooltip.

Turning an adapter off disconnects that interface and stops NetworkManager
from managing it. Other Wi-Fi adapters remain enabled. Turning it back on
restores management and allows automatic connections. The header Wi-Fi switch
continues to control the system-wide radio, independently of these switches.
An adapter can be enabled while the global Wi-Fi radio is off.

The available-network list follows a managed adapter, preferring an existing
connection, then a connection in progress. It keeps the current adapter when
otherwise equivalent adapters are present. Disabling the built-in adapter
lets the network list use the external adapter. Unplugging an adapter updates
the list and falls back to another managed adapter if available. If every
adapter is disabled, enable one in this section to use it again.

These switches control NetworkManager management, not a hardware power switch.
The setting lasts until NetworkManager restarts; it is not written to shared
Wayle configuration. Permission failures are shown in the dropdown.

The fork pins `wayle-network` 0.1.2 in `vendor/wayle-network` because the upstream
service exposed only one Wi-Fi device. This copy adds adapter enumeration and
selection while preserving the original service API.
