slint::include_modules!();

use tokio::process::Command;
use tokio::time::{sleep, Duration};
use std::collections::HashMap;
use slint::{ModelRc, VecModel, Weak};
use std::rc::Rc;

#[derive(Debug, Clone)]
struct InternalNetwork {
    ssid: String,
    signal: i32,
    security: String,
    in_use: bool,
    channel: String,
}

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    if std::env::var("SLINT_BACKEND").is_err() {
        std::env::set_var("SLINT_BACKEND", "winit");
    }

    let ui = AppWindow::new()?;
    let ui_weak = ui.as_weak();

    // Callbacks
    ui.on_exit(|| std::process::exit(0));

    let ui_weak_toggle = ui_weak.clone();
    ui.on_toggle_wifi(move |enable| {
        let ui = ui_weak_toggle.clone();
        tokio::spawn(async move {
            let state = if enable { "on" } else { "off" };
            let _ = Command::new("nmcli").args(["radio", "wifi", state]).status().await;
            if enable { sleep(Duration::from_secs(2)).await; }
            refresh_status(ui).await;
        });
    });

    let ui_weak_scan = ui_weak.clone();
    ui.on_scan_networks(move || {
        let ui = ui_weak_scan.clone();
        tokio::spawn(async move {
            update_busy_status(&ui, true, "Scanning...").await;
            let networks = scan_networks_cmd().await;
            update_networks(&ui, networks).await;
            update_busy_status(&ui, false, "").await;
        });
    });

    let ui_weak_connect = ui_weak.clone();
    ui.on_connect_network(move |ssid| {
        let ui = ui_weak_connect.clone();
        let ssid_str = ssid.to_string();
        tokio::spawn(async move {
            update_busy_status(&ui, true, &format!("Connecting to {}...", ssid_str)).await;
            match connect_network_cmd(ssid_str.clone(), None).await {
                Ok(_) => {
                    notify(&ui, "Connected successfully", true).await;
                    let networks = scan_networks_cmd().await;
                    update_networks(&ui, networks).await;
                }
                Err(e) => {
                    if e.contains("secrets") || e.contains("password") {
                        let ui_copy = ui.clone();
                        let ssid_copy = ssid_str.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_copy.upgrade() {
                                ui.set_password_ssid(ssid_copy.into());
                                ui.set_password_input("".into());
                                ui.set_show_password_modal(true);
                            }
                        }).unwrap();
                    } else {
                        notify(&ui, &format!("Connection failed: {}", e), false).await;
                    }
                }
            }
            update_busy_status(&ui, false, "").await;
        });
    });

    let ui_weak_pass = ui_weak.clone();
    ui.on_submit_password(move |ssid, pass| {
        let ui = ui_weak_pass.clone();
        let ssid_str = ssid.to_string();
        let pass_str = pass.to_string();
        tokio::spawn(async move {
            update_busy_status(&ui, true, &format!("Connecting to {}...", ssid_str)).await;
            match connect_network_cmd(ssid_str, Some(pass_str)).await {
                Ok(_) => {
                    notify(&ui, "Connected successfully", true).await;
                    let networks = scan_networks_cmd().await;
                    update_networks(&ui, networks).await;
                }
                Err(e) => {
                    notify(&ui, &format!("Connection failed: {}", e), false).await;
                }
            }
            update_busy_status(&ui, false, "").await;
        });
    });

    let ui_weak_disc = ui_weak.clone();
    ui.on_disconnect_network(move || {
        let ui = ui_weak_disc.clone();
        tokio::spawn(async move {
            let iface = get_wireless_interface().await;
            let _ = Command::new("nmcli").args(["dev", "disconnect", &iface]).output().await;
            notify(&ui, "Disconnected", true).await;
            let networks = scan_networks_cmd().await;
            update_networks(&ui, networks).await;
        });
    });

    let ui_weak_forget = ui_weak.clone();
    ui.on_forget_network(move |ssid| {
        let ui = ui_weak_forget.clone();
        let ssid_str = ssid.to_string();
        tokio::spawn(async move {
            let _ = Command::new("nmcli").args(["connection", "delete", &ssid_str]).output().await;
            notify(&ui, "Network forgotten", true).await;
            let networks = scan_networks_cmd().await;
            update_networks(&ui, networks).await;
            let ui_copy = ui.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_copy.upgrade() {
                    ui.set_show_advanced(false);
                }
            }).unwrap();
        });
    });

    let ui_weak_adv = ui_weak.clone();
    ui.on_open_advanced(move |ssid| {
        let ui = ui_weak_adv.clone();
        let ssid_str = ssid.to_string();
        tokio::spawn(async move {
            update_busy_status(&ui, true, "Loading details...").await;
            let networks = scan_networks_cmd().await;
            let net = networks.iter().find(|n| n.ssid == ssid_str).cloned();
            let settings = get_connection_settings(&ssid_str).await;
            
            if let Some(n) = net {
                let ui_copy = ui.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_copy.upgrade() {
                        ui.set_current_network(NetworkInfo {
                            ssid: n.ssid.into(),
                            signal: n.signal,
                            security: n.security.into(),
                            in_use: n.in_use,
                            channel: n.channel.into(),
                        });
                        ui.set_ip_address(settings.0.into());
                        ui.set_gateway(settings.1.into());
                        ui.set_dns(settings.2.into());
                        ui.set_show_advanced(true);
                    }
                }).unwrap();
                refresh_devices(ui.clone()).await;
            }
            update_busy_status(&ui, false, "").await;
        });
    });

    ui.on_close_advanced({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_show_advanced(false);
            }
        }
    });

    let ui_weak_refresh = ui_weak.clone();
    ui.on_refresh_devices(move || {
        let ui = ui_weak_refresh.clone();
        tokio::spawn(async move {
            refresh_devices(ui).await;
        });
    });

    let ui_weak_apply = ui_weak.clone();
    ui.on_apply_settings(move |ssid, ip, gw, dns| {
        let ui = ui_weak_apply.clone();
        let ssid_str = ssid.to_string();
        let ip_str = ip.to_string();
        let gw_str = gw.to_string();
        let dns_str = dns.to_string();
        
        tokio::spawn(async move {
            update_busy_status(&ui, true, "Applying settings...").await;
            let res = {
                if !ip_str.is_empty() {
                    // Add prefix if missing (default to /24)
                    let ip_with_prefix = if ip_str.contains('/') { ip_str.clone() } else { format!("{}/24", ip_str) };
                    let mut args = vec!["connection", "modify", &ssid_str, "ipv4.method", "manual", "ipv4.addresses", &ip_with_prefix];
                    if !gw_str.is_empty() {
                        args.push("ipv4.gateway");
                        args.push(&gw_str);
                    }
                    if !dns_str.is_empty() {
                        args.push("ipv4.dns");
                        args.push(&dns_str);
                    }
                    let _ = Command::new("nmcli").args(&args).output().await;
                } else {
                    let _ = Command::new("nmcli").args(["connection", "modify", &ssid_str, "ipv4.method", "auto"]).output().await;
                }
                Command::new("nmcli").args(["connection", "up", &ssid_str]).output().await
            };

            match res {
                Ok(_) => notify(&ui, "Settings applied", true).await,
                Err(e) => notify(&ui, &format!("Failed: {}", e), false).await,
            }
            update_busy_status(&ui, false, "").await;
        });
    });

    ui.on_copy_ip(move |ip| {
        let ip_str = ip.to_string();
        tokio::spawn(async move {
            if Command::new("wl-copy").arg(&ip_str).status().await.is_err() {
                use std::process::Stdio;
                use tokio::io::AsyncWriteExt;
                let child = Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(Stdio::piped())
                    .spawn()
                    .ok();
                
                if let Some(mut child) = child {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(ip_str.as_bytes()).await;
                    }
                    let _ = child.wait().await;
                }
            }
        });
    });

    // Run initial scan
    let ui_initial = ui_weak.clone();
    tokio::spawn(async move {
        refresh_status(ui_initial).await;
    });

    ui.run()
}

async fn connect_network_cmd(ssid: String, password: Option<String>) -> Result<(), String> {
    let mut args = vec!["dev", "wifi", "connect", &ssid];
    if let Some(ref p) = password {
        args.push("password");
        args.push(p);
    }
    
    let output = Command::new("nmcli").args(&args).output().await.map_err(|e| e.to_string())?;
    if output.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&output.stderr).to_string()) }
}

async fn refresh_status(ui: Weak<AppWindow>) {
    let output = Command::new("nmcli").args(["radio", "wifi"]).output().await.ok();
    let enabled = if let Some(out) = output {
        String::from_utf8_lossy(&out.stdout).trim() == "enabled"
    } else {
        false
    };

    let ui_copy = ui.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            ui.set_wifi_enabled(enabled);
        }
    }).unwrap();

    if enabled {
        let networks = scan_networks_cmd().await;
        update_networks(&ui, networks).await;
    }
}

async fn update_networks(ui: &Weak<AppWindow>, networks: Vec<InternalNetwork>) {
    println!("Updating UI with {} networks", networks.len());
    let slint_networks: Vec<NetworkInfo> = networks.into_iter().map(|n| NetworkInfo {
        ssid: n.ssid.into(),
        signal: n.signal,
        security: n.security.into(),
        in_use: n.in_use,
        channel: n.channel.into(),
    }).collect();

    let ui_copy = ui.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            let model = VecModel::from(slint_networks);
            ui.set_networks(ModelRc::from(Rc::new(model)));
        }
    }).unwrap();
}

async fn update_busy_status(ui: &Weak<AppWindow>, busy: bool, msg: &str) {
    let ui_copy = ui.clone();
    let msg_str = msg.to_string();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            ui.set_is_busy(busy);
            ui.set_busy_message(msg_str.into());
        }
    }).unwrap();
}

async fn notify(ui: &Weak<AppWindow>, msg: &str, success: bool) {
    let ui_copy = ui.clone();
    let msg_str = msg.to_string();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            ui.set_notification_message(msg_str.into());
            ui.set_notification_success(success);
        }
    }).unwrap();
}

async fn scan_networks_cmd() -> Vec<InternalNetwork> {
    println!("Starting nmcli scan...");
    let mut output = Command::new("nmcli")
        .args(["-t", "-f", "SSID,SIGNAL,SECURITY,IN-USE,CHAN", "dev", "wifi", "list"])
        .output()
        .await
        .ok();

    if let Some(ref out) = output {
        println!("nmcli output length: {}", out.stdout.len());
        if out.stdout.is_empty() {
            println!("Triggering rescan...");
            let _ = Command::new("nmcli").args(["dev", "wifi", "rescan"]).output().await;
            sleep(Duration::from_millis(1500)).await;
            output = Command::new("nmcli")
                .args(["-t", "-f", "SSID,SIGNAL,SECURITY,IN-USE,CHAN", "dev", "wifi", "list"])
                .output()
                .await
                .ok();
        }
    }

    let mut networks_map: HashMap<String, InternalNetwork> = HashMap::new();

    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            println!("Parsing line: {}", line);
            let placeholder = "___COLON___";
            let escaped_line = line.replace("\\:", placeholder);
            let parts: Vec<String> = escaped_line.split(':')
                .map(|s| s.replace(placeholder, ":"))
                .collect();

            if parts.len() >= 5 {
                let mut ssid = parts[0].clone();
                if ssid.is_empty() { ssid = "<Hidden>".into(); }
                let signal = parts[1].parse::<i32>().unwrap_or(0);
                let security = parts[2].clone();
                let in_use = parts[3].trim() == "*";
                let channel = parts[4].clone();
                
                let net = InternalNetwork {
                    ssid: ssid.clone(),
                    signal,
                    security,
                    in_use,
                    channel,
                };

                networks_map.entry(ssid)
                    .and_modify(|e| {
                        if in_use || (!e.in_use && e.signal < signal) {
                            *e = net.clone();
                        }
                    })
                    .or_insert(net);
            } else {
                println!("Line skipped (parts len: {})", parts.len());
            }
        }
    } else {
        println!("No output from nmcli");
    }

    let mut result: Vec<InternalNetwork> = networks_map.into_values().collect();
    println!("Found {} unique networks", result.len());
    result.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    result
}

async fn get_wireless_interface() -> String {
    let output = Command::new("nmcli").args(["-t", "-f", "DEVICE,TYPE", "device"]).output().await.ok();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 && parts[1] == "wifi" {
                return parts[0].to_string();
            }
        }
    }
    "wlan0".to_string()
}

async fn get_connection_settings(ssid: &str) -> (String, String, String) {
    let output = Command::new("nmcli").args(["-t", "-f", "ipv4.addresses,ipv4.gateway,ipv4.dns", "connection", "show", ssid]).output().await.ok();
    let mut ip = String::new();
    let mut gw = String::new();
    let mut dns = String::new();
    
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                let val = if parts[1] == "--" { "" } else { parts[1] };
                match parts[0] {
                    "ipv4.addresses" => ip = val.to_string(),
                    "ipv4.gateway" => gw = val.to_string(),
                    "ipv4.dns" => dns = val.to_string(),
                    _ => {}
                }
            }
        }
    }
    (ip, gw, dns)
}

async fn get_local_ip_and_subnet() -> Option<String> {
    let iface = get_wireless_interface().await;
    // nmcli -g ip4.address device show wlan0
    // returns something like 192.168.1.10/24
    let output = Command::new("nmcli").args(["-g", "ip4.address", "device", "show", &iface]).output().await.ok();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !stdout.is_empty() {
            return Some(stdout);
        }
    }
    None
}

async fn refresh_devices(ui: Weak<AppWindow>) {
    let ui_copy = ui.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            ui.set_loading_devices(true);
        }
    }).unwrap();

    let mut devices_map: HashMap<String, ConnectedDevice> = HashMap::new();
    if let Some(subnet) = get_local_ip_and_subnet().await {
        let output = Command::new("nmap").args(["-sn", "--unprivileged", &subnet]).output().await.ok();
        
        if let Some(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut current_ip = String::new();
            let mut current_hostname = String::new();
            
            for line in stdout.lines() {
                if line.contains("Nmap scan report for") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let last = parts.last().unwrap();
                        if last.starts_with('(') && last.ends_with(')') {
                            current_ip = last.trim_matches('(').trim_matches(')').to_string();
                            current_hostname = parts[4..parts.len()-1].join(" ");
                        } else {
                            current_ip = last.to_string();
                            current_hostname = "".to_string();
                        }
                    } else if parts.len() == 5 {
                        current_ip = parts[4].to_string();
                        current_hostname = "".to_string();
                    }
                } else if line.contains("MAC Address:") && !current_ip.is_empty() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let mac = parts[2].to_string();
                        let vendor = parts[3..].join(" ").trim_matches('(').trim_matches(')').to_string();
                        devices_map.insert(current_ip.clone(), ConnectedDevice {
                            hostname: current_hostname.clone().into(),
                            mac: mac.into(),
                            ip: current_ip.clone().into(),
                            vendor: vendor.into(),
                        });
                        current_ip.clear();
                        current_hostname.clear();
                    }
                } else if line.contains("Host is up")
                    && !current_ip.is_empty()
                    && !devices_map.contains_key(&current_ip)
                {
                    devices_map.insert(current_ip.clone(), ConnectedDevice {
                        hostname: current_hostname.clone().into(),
                        mac: "Unknown".into(),
                        ip: current_ip.clone().into(),
                        vendor: "".into(),
                    });
                }
            }
        }
    }

    // Supplementary scan via 'ip neighbor' to get MACs that unprivileged nmap might miss
    let iface = get_wireless_interface().await;
    let neigh_output = Command::new("ip").args(["neighbor", "show", "dev", &iface]).output().await.ok();
    if let Some(nout) = neigh_output {
        let nstdout = String::from_utf8_lossy(&nout.stdout);
        for nline in nstdout.lines() {
            let nparts: Vec<&str> = nline.split_whitespace().collect();
            if nparts.len() >= 4 {
                let ip = nparts[0].to_string();
                if let Some(ll_idx) = nparts.iter().position(|&s| s == "lladdr") {
                    if ll_idx + 1 < nparts.len() {
                        let mac = nparts[ll_idx + 1].to_string();
                        let state = nparts.last().unwrap_or(&"");
                        if *state == "FAILED" { continue; }

                        devices_map.entry(ip.clone())
                            .and_modify(|d| {
                                if d.mac == "Unknown" {
                                    d.mac = mac.clone().into();
                                }
                            })
                            .or_insert(ConnectedDevice {
                                hostname: "".into(),
                                mac: mac.into(),
                                ip: ip.into(),
                                vendor: "".into(),
                            });
                    }
                }
            }
        }
    }

    let mut devices: Vec<ConnectedDevice> = devices_map.into_values().collect();
    devices.sort_by(|a, b| {
        let a_parts: Vec<u32> = a.ip.split('.').map(|s| s.parse().unwrap_or(0)).collect();
        let b_parts: Vec<u32> = b.ip.split('.').map(|s| s.parse().unwrap_or(0)).collect();
        a_parts.cmp(&b_parts)
    });

    let ui_copy = ui.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_copy.upgrade() {
            let model = VecModel::from(devices);
            ui.set_connected_devices(ModelRc::from(Rc::new(model)));
            ui.set_loading_devices(false);
        }
    }).unwrap();
}
