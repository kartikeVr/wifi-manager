# 📶 Wi-Fi Manager (Slint + Rust)

A modern, neumorphic Wi-Fi management tool built with **Rust** and the **Slint UI framework**. It provides a lightweight alternative to heavy network managers, featuring a clean interface for scanning, connecting, and inspecting network devices.

## 🚀 One-Liner Installation

Run this command to build and run the application immediately (requires Rust/Cargo):

```bash
git clone https://github.com/kartikeVr/wifi-manager.git && cd wifi-manager && cargo run --release
```

*Note: Ensure `nmcli`, `nmap`, and `ip` tools are installed on your Linux system.*

---

## 🏗️ Architecture

The project follows a decoupled **Frontend-Backend** architecture using the Slint event loop:

### 1. **UI Layer (Slint)**
- **Neumorphic Design:** Custom-built UI components in `ui/app.slint` using a modern "Soft UI" aesthetic.
- **Reactive Properties:** Uses Slint properties to sync network lists, connection status, and busy indicators in real-time.
- **Advanced View:** Contextual right-click/long-press menu for IP configuration and network discovery.

### 2. **Logic Layer (Rust/Tokio)**
- **Asynchronous Execution:** Powered by `tokio`, ensuring the UI remains responsive even during long-running network scans.
- **Command Orchestration:** Interfaces directly with system binaries (`nmcli`, `nmap`, `ip`) via `std::process::Command`.
- **Dynamic Interface Detection:** Automatically detects the active wireless interface (e.g., `wlan0`, `wlp2s0`) instead of using hardcoded values.

---

## 🔄 Workflow

1.  **Discovery:** On startup, the app triggers an `nmcli` scan. If no networks are found, it automatically initiates a `dev wifi rescan`.
2.  **Interaction:**
    *   **Connect:** Clicking a network opens a password prompt if security is detected.
    *   **Advanced Info:** Right-clicking a network shows detailed IP settings (Gateway, DNS, Subnet).
3.  **Network Mapping:** When connected, the app uses `nmap` and `ip neighbor` to map all other devices on the current subnet, identifying vendors and MAC addresses.
4.  **Clipboard Integration:** Supports one-click IP copying for both **Wayland** (`wl-copy`) and **X11** (`xclip`).

---

## 🛠️ Requirements

- **Rust** (Edition 2021)
- **NetworkManager** (`nmcli`)
- **nmap** (for network device discovery)
- **iproute2** (`ip`)
- **wl-clipboard** or **xclip** (for copy functionality)

---

## 📸 Features
- ✅ Real-time signal strength indicators.
- ✅ Support for hidden SSIDs.
- ✅ Static IP/DNS configuration.
- ✅ Subnet device scanner (Vendor & MAC detection).
- ✅ Dark-themed Neumorphic UI.
