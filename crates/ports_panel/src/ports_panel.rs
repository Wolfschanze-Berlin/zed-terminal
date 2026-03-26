use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context as _;
use gpui::{
    Action, App, AsyncApp, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Subscription, WeakEntity, actions,
};
use parking_lot::Mutex;
use remote::RemoteConnectionOptions;
use settings_content::SshPortForwardOption;
use ssh_panel::{ConnectionState, DetectedPort, SshPanel, SshPanelEvent};
use ui::{Tooltip, prelude::*};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(ports_panel, [ToggleFocus, TogglePanel, AddForward, RemoveForward]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<PortsPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &TogglePanel, window, cx| {
            if !workspace.toggle_panel_focus::<PortsPanel>(window, cx) {
                workspace.close_panel::<PortsPanel>(window, cx);
            }
        });
    })
    .detach();
}

#[derive(Clone, Debug, PartialEq)]
enum ForwardStatus {
    Active,
    Inactive,
    Starting,
    Failed(String),
}

#[derive(Clone, Debug)]
struct PortForwardEntry {
    option: SshPortForwardOption,
    host_name: String,
    status: ForwardStatus,
}

/// Tracks running SSH tunnel processes. Each tunnel is an `ssh -N -L ...` subprocess.
struct TunnelProcess {
    child: Arc<Mutex<Option<util::command::Child>>>,
}

impl TunnelProcess {
    fn kill(&self) {
        if let Some(mut child) = self.child.lock().take() {
            let _ = child.kill();
        }
    }
}

impl Drop for TunnelProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Unique key for a port forward entry
fn forward_key(host: &str, local_port: u16, remote_port: u16) -> String {
    format!("{}:{}:{}", host, local_port, remote_port)
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedPortForwards {
    /// Map from SSH host name to list of saved forwards
    hosts: collections::HashMap<String, Vec<SshPortForwardOption>>,
}

fn saved_forwards_path() -> PathBuf {
    paths::config_dir().join("port_forwards.json")
}

fn load_saved_forwards() -> SavedPortForwards {
    let path = saved_forwards_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => SavedPortForwards::default(),
    }
}

fn save_forwards(forwards: &[PortForwardEntry]) {
    let mut saved = SavedPortForwards::default();
    for entry in forwards {
        saved
            .hosts
            .entry(entry.host_name.clone())
            .or_default()
            .push(entry.option.clone());
    }
    if let Ok(json) = serde_json::to_string_pretty(&saved) {
        let path = saved_forwards_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, json);
    }
}

/// Interval between automatic port scans on the remote host
const PORT_SCAN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

pub struct PortsPanel {
    focus_handle: FocusHandle,
    width: Option<Pixels>,
    forwards: Vec<PortForwardEntry>,
    tunnels: collections::HashMap<String, TunnelProcess>,
    show_add_form: bool,
    form_local_port: String,
    form_remote_host: String,
    form_remote_port: String,
    selected_host: Option<String>,
    detected_ports: collections::HashMap<String, Vec<DetectedPort>>,
    auto_forward: bool,
    ssh_panel: Option<Entity<SshPanel>>,
    _poll_task: Option<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl PortsPanel {
    pub fn new(
        ssh_panel: Option<Entity<SshPanel>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = Vec::new();

        if let Some(ref panel) = ssh_panel {
            subscriptions.push(cx.subscribe(panel, Self::on_ssh_panel_event));
        }

        // Load saved forwards from disk
        let saved = load_saved_forwards();
        let forwards: Vec<PortForwardEntry> = saved
            .hosts
            .into_iter()
            .flat_map(|(host_name, options)| {
                options.into_iter().map(move |option| PortForwardEntry {
                    option,
                    host_name: host_name.clone(),
                    status: ForwardStatus::Inactive,
                })
            })
            .collect();

        Self {
            focus_handle: cx.focus_handle(),
            width: None,
            forwards,
            tunnels: collections::HashMap::default(),
            detected_ports: collections::HashMap::default(),
            auto_forward: true,
            show_add_form: false,
            form_local_port: String::new(),
            form_remote_host: String::new(),
            form_remote_port: String::new(),
            selected_host: None,
            ssh_panel,
            _poll_task: None,
            _subscriptions: subscriptions,
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, _window, cx| {
            let ssh_panel = workspace.panel::<SshPanel>(cx);

            // Check if this is a remote workspace to pre-select the host
            let remote_host_name = workspace
                .project()
                .read(cx)
                .remote_client()
                .and_then(|client| {
                    let opts = client.read(cx).connection_options();
                    if let RemoteConnectionOptions::Ssh(ssh_opts) = &opts {
                        ssh_opts
                            .nickname
                            .clone()
                            .unwrap_or_else(|| ssh_opts.host.to_string())
                            .into()
                    } else {
                        None
                    }
                });

            cx.new(|cx| {
                let mut panel = Self::new(ssh_panel, cx);
                // If already connected (e.g., remote window), set host and start polling
                let connected = panel.connected_hosts(cx);
                if !connected.is_empty() {
                    // Prefer the remote host name from the connection, fall back to first connected
                    panel.selected_host = remote_host_name
                        .and_then(|name| {
                            connected
                                .iter()
                                .find(|h| **h == name)
                                .cloned()
                        })
                        .or_else(|| connected.into_iter().next());
                    panel.start_port_polling(cx);
                }
                panel
            })
        })
    }

    fn on_ssh_panel_event(
        &mut self,
        _ssh_panel: Entity<SshPanel>,
        event: &SshPanelEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SshPanelEvent::Connected(host) => {
                if self.selected_host.is_none() {
                    self.selected_host = Some(host.name.clone());
                }
                self.auto_start_forwards_for_host(&host.name, cx);
                self.start_port_polling(cx);
            }
            SshPanelEvent::Disconnected(host) => {
                self.stop_forwards_for_host(&host.name, cx);
                if self.selected_host.as_deref() == Some(&host.name) {
                    self.selected_host = self.connected_hosts(cx).into_iter().next();
                }
                // Stop polling if no hosts remain connected
                if self.connected_hosts(cx).is_empty() {
                    self._poll_task = None;
                }
            }
            SshPanelEvent::RemotePortsDetected { host_name, ports } => {
                self.handle_detected_ports_and_auto_forward(host_name, ports, cx);
            }
        }
        cx.notify();
    }

    fn connected_hosts(&self, _cx: &Context<Self>) -> Vec<String> {
        let Some(ref ssh_panel) = self.ssh_panel else {
            return Vec::new();
        };
        ssh_panel
            .read(_cx)
            .connection_store()
            .lock()
            .connected_hosts()
    }

    fn auto_start_forwards_for_host(&mut self, host_name: &str, cx: &mut Context<Self>) {
        let indices: Vec<usize> = self
            .forwards
            .iter()
            .enumerate()
            .filter(|(_, f)| f.host_name == host_name && f.status == ForwardStatus::Inactive)
            .map(|(i, _)| i)
            .collect();

        for index in indices {
            self.start_forward(index, cx);
        }
    }

    fn stop_forwards_for_host(&mut self, host_name: &str, cx: &mut Context<Self>) {
        for entry in &mut self.forwards {
            if entry.host_name == host_name {
                let key = forward_key(
                    &entry.host_name,
                    entry.option.local_port,
                    entry.option.remote_port,
                );
                if let Some(tunnel) = self.tunnels.remove(&key) {
                    tunnel.kill();
                }
                entry.status = ForwardStatus::Inactive;
            }
        }
        cx.notify();
    }

    fn start_port_polling(&mut self, cx: &mut Context<Self>) {
        // Don't start if already polling
        if self._poll_task.is_some() {
            return;
        }

        self._poll_task = Some(cx.spawn(async move |this, cx: &mut AsyncApp| {
            loop {
                cx.background_spawn(async {
                    smol::Timer::after(PORT_SCAN_INTERVAL).await;
                })
                .await;

                let should_continue = this
                    .update(cx, |this, cx| {
                        if this.connected_hosts(cx).is_empty() {
                            return false;
                        }
                        this.scan_remote_ports(cx);
                        true
                    })
                    .unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        }));
    }

    fn handle_detected_ports_and_auto_forward(
        &mut self,
        host_name: &str,
        ports: &[DetectedPort],
        cx: &mut Context<Self>,
    ) {
        let previous = self.detected_ports.get(host_name).cloned();
        self.detected_ports
            .insert(host_name.to_string(), ports.to_vec());

        if !self.auto_forward {
            return;
        }

        // Find newly appeared ports (not in previous scan)
        let previous_ports: std::collections::HashSet<u16> = previous
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|d| d.port)
            .collect();

        let already_forwarded: std::collections::HashSet<u16> = self
            .forwards
            .iter()
            .filter(|f| f.host_name == host_name)
            .map(|f| f.option.remote_port)
            .collect();

        let new_ports: Vec<u16> = ports
            .iter()
            .filter(|d| !previous_ports.contains(&d.port) && !already_forwarded.contains(&d.port))
            .map(|d| d.port)
            .collect();

        for port in new_ports {
            log::info!("Auto-forwarding newly detected port {} on {}", port, host_name);
            self.add_detected_forward(host_name, port, cx);
        }
    }

    fn add_detected_forward(
        &mut self,
        host_name: &str,
        port: u16,
        cx: &mut Context<Self>,
    ) {
        let option = SshPortForwardOption {
            local_host: None,
            local_port: port,
            remote_host: None,
            remote_port: port,
        };

        self.forwards.push(PortForwardEntry {
            option,
            host_name: host_name.to_string(),
            status: ForwardStatus::Inactive,
        });

        let index = self.forwards.len() - 1;

        let is_connected = self
            .ssh_panel
            .as_ref()
            .map(|p| {
                p.read(cx)
                    .connection_store()
                    .lock()
                    .state(host_name)
                    == ConnectionState::Connected
            })
            .unwrap_or(false);

        if is_connected {
            self.start_forward(index, cx);
        }
        save_forwards(&self.forwards);
        cx.notify();
    }

    fn scan_remote_ports(&mut self, cx: &mut Context<Self>) {
        let Some(ref ssh_panel) = self.ssh_panel else {
            return;
        };
        let Some(host_name) = self.selected_host.clone() else {
            return;
        };

        // Find host index in SSH panel
        let host_index = ssh_panel.read(cx).hosts().iter().position(|h| h.name == host_name);
        if let Some(index) = host_index {
            ssh_panel.update(cx, |panel, cx| {
                panel.detect_remote_ports(index, cx);
            });
        } else {
            // Host not in SSH config (e.g., connected via raw IP from remote window).
            // Run detection using the host name directly as SSH destination.
            let host_for_scan = host_name.clone();
            cx.spawn(async move |this, cx: &mut AsyncApp| {
                let detected = cx
                    .background_spawn(async move {
                        let mut cmd = util::command::new_command("ssh");
                        cmd.arg(&host_for_scan);
                        cmd.arg("ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null");
                        cmd.stdout(util::command::Stdio::piped());
                        cmd.stderr(util::command::Stdio::null());

                        let output = cmd.output().await?;
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        anyhow::Ok(ssh_panel::parse_listening_ports(&stdout))
                    })
                    .await?;

                this.update(cx, |this, cx| {
                    this.handle_detected_ports_and_auto_forward(&host_name, &detected, cx);
                    cx.notify();
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }
    }

    fn toggle_add_form(&mut self, cx: &mut Context<Self>) {
        self.show_add_form = !self.show_add_form;
        if self.show_add_form {
            self.form_local_port.clear();
            self.form_remote_host.clear();
            self.form_remote_port.clear();
        }
        cx.notify();
    }

    fn add_forward_from_form(&mut self, cx: &mut Context<Self>) {
        let local_port = match self.form_local_port.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                log::warn!("Invalid local port: {}", self.form_local_port);
                return;
            }
        };

        let remote_port = match self.form_remote_port.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                log::warn!("Invalid remote port: {}", self.form_remote_port);
                return;
            }
        };

        let Some(host_name) = self.selected_host.clone() else {
            log::warn!("No host selected for port forward");
            return;
        };

        let remote_host = if self.form_remote_host.is_empty() {
            None
        } else {
            Some(self.form_remote_host.clone())
        };

        let option = SshPortForwardOption {
            local_host: None,
            local_port,
            remote_host,
            remote_port,
        };

        let entry = PortForwardEntry {
            option,
            host_name: host_name.clone(),
            status: ForwardStatus::Inactive,
        };

        self.forwards.push(entry);
        let index = self.forwards.len() - 1;

        self.show_add_form = false;
        self.form_local_port.clear();
        self.form_remote_host.clear();
        self.form_remote_port.clear();

        // Auto-start if host is connected
        let is_connected = self
            .ssh_panel
            .as_ref()
            .map(|p| {
                p.read(cx)
                    .connection_store()
                    .lock()
                    .state(&host_name)
                    == ConnectionState::Connected
            })
            .unwrap_or(false);

        if is_connected {
            self.start_forward(index, cx);
        }

        save_forwards(&self.forwards);
        cx.notify();
    }

    fn start_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.forwards.get_mut(index) else {
            return;
        };

        let host_name = entry.host_name.clone();
        let local_port = entry.option.local_port;
        let remote_host = entry
            .option
            .remote_host
            .as_deref()
            .unwrap_or("localhost");
        let remote_port = entry.option.remote_port;

        // Look up the SSH config entry, or fall back to using the host name directly
        let ssh_config_host = self
            .ssh_panel
            .as_ref()
            .and_then(|p| p.read(cx).host_by_name(&host_name).cloned());

        let ssh_destination = ssh_config_host
            .as_ref()
            .map(|h| h.name.clone())
            .unwrap_or_else(|| host_name.clone());
        let ssh_port = ssh_config_host.as_ref().and_then(|h| h.port);

        entry.status = ForwardStatus::Starting;
        cx.notify();

        let forward_spec = format!(
            "{}:{}:{}",
            local_port, remote_host, remote_port
        );

        let key = forward_key(&host_name, local_port, remote_port);
        let key_for_task = key.clone();
        let host_name_for_task = host_name.clone();

        let child_arc: Arc<Mutex<Option<util::command::Child>>> = Arc::new(Mutex::new(None));
        let child_arc_clone = child_arc.clone();

        self.tunnels.insert(
            key,
            TunnelProcess {
                child: child_arc.clone(),
            },
        );

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let spawn_result = cx.background_spawn(async move {
                let mut cmd = util::command::new_command("ssh");
                cmd.arg("-N"); // no remote command
                cmd.arg("-L");
                cmd.arg(&forward_spec);

                if let Some(port) = ssh_port {
                    cmd.arg("-p");
                    cmd.arg(port.to_string());
                }

                // -o ExitOnForwardFailure=yes ensures SSH exits if the tunnel can't bind
                cmd.arg("-o");
                cmd.arg("ExitOnForwardFailure=yes");

                cmd.arg(&ssh_destination);

                cmd.stdin(util::command::Stdio::null());
                cmd.stdout(util::command::Stdio::null());
                cmd.stderr(util::command::Stdio::piped());

                let child = cmd.spawn().context("Failed to spawn SSH tunnel process")?;
                anyhow::Ok(child)
            })
            .await;

            match spawn_result {
                Ok(child) => {
                    *child_arc_clone.lock() = Some(child);

                    // Brief delay to detect immediate bind failures
                    cx.background_spawn(async {
                        smol::Timer::after(std::time::Duration::from_millis(500)).await;
                    })
                    .await;

                    // Check if process exited immediately (bind failure)
                    let still_running = child_arc_clone
                        .lock()
                        .as_mut()
                        .map(|c| c.try_status().ok().flatten().is_none())
                        .unwrap_or(false);

                    this.update(cx, |this, cx| {
                        if let Some(entry) = this.forwards.iter_mut().find(|f| {
                            forward_key(&f.host_name, f.option.local_port, f.option.remote_port)
                                == key_for_task
                        }) {
                            if still_running {
                                entry.status = ForwardStatus::Active;
                            } else {
                                entry.status = ForwardStatus::Failed(
                                    "Port may already be in use".to_string(),
                                );
                                this.tunnels.remove(&key_for_task);
                            }
                        }
                        cx.notify();
                    })?;
                }
                Err(err) => {
                    this.update(cx, |this, cx| {
                        if let Some(entry) = this.forwards.iter_mut().find(|f| {
                            f.host_name == host_name_for_task
                        }) {
                            entry.status =
                                ForwardStatus::Failed(format!("{}", err));
                        }
                        this.tunnels.remove(&key_for_task);
                        cx.notify();
                    })?;
                }
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn stop_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.forwards.get_mut(index) else {
            return;
        };

        let key = forward_key(
            &entry.host_name,
            entry.option.local_port,
            entry.option.remote_port,
        );

        if let Some(tunnel) = self.tunnels.remove(&key) {
            tunnel.kill();
        }
        entry.status = ForwardStatus::Inactive;
        cx.notify();
    }

    fn remove_forward(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.forwards.len() {
            let entry = &self.forwards[index];
            let key = forward_key(
                &entry.host_name,
                entry.option.local_port,
                entry.option.remote_port,
            );
            if let Some(tunnel) = self.tunnels.remove(&key) {
                tunnel.kill();
            }
            self.forwards.remove(index);
            save_forwards(&self.forwards);
            cx.notify();
        }
    }

    pub fn port_forward_options(&self) -> Vec<SshPortForwardOption> {
        self.forwards
            .iter()
            .map(|entry| entry.option.clone())
            .collect()
    }
}

impl Focusable for PortsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PortsPanel {}

impl Panel for PortsPanel {
    fn persistent_name() -> &'static str {
        "PortsPanel"
    }

    fn panel_key() -> &'static str {
        "PortsPanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, _position: DockPosition, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn size(&self, _window: &Window, _cx: &App) -> Pixels {
        self.width.unwrap_or(px(300.))
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.width = size;
        cx.notify();
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::Link)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Port Forwards")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        5
    }
}

impl Render for PortsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_forwards = !self.forwards.is_empty();
        let show_form = self.show_add_form;
        let connected_hosts = self.connected_hosts(cx);
        let has_connections = !connected_hosts.is_empty();

        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new("Port Forwards")
                    .size(LabelSize::Default)
                    .color(Color::Default),
            )
            .when(has_connections, |el| {
                let auto_forward = self.auto_forward;
                el.child(
                    h_flex()
                        .gap_1()
                        .child(
                            IconButton::new(
                                "toggle-auto-forward",
                                if auto_forward {
                                    IconName::BoltFilled
                                } else {
                                    IconName::BoltOutlined
                                },
                            )
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(if auto_forward {
                                "Auto-forward: ON"
                            } else {
                                "Auto-forward: OFF"
                            }))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.auto_forward = !this.auto_forward;
                                cx.notify();
                            })),
                        )
                        .child(
                            IconButton::new("scan-ports", IconName::MagnifyingGlass)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Scan Remote Ports"))
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.scan_remote_ports(cx);
                                })),
                        )
                        .child(
                            IconButton::new("add-forward", IconName::Plus)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Add Forward"))
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.toggle_add_form(cx);
                                })),
                        ),
                )
            });

        let mut panel = v_flex()
            .key_context("PortsPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(header);

        if !has_connections {
            return panel
                .child(
                    div()
                        .p_4()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .child(
                            Label::new("Connect to an SSH host to manage port forwarding.")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .into_any_element();
        }

        // Host selector when multiple hosts are connected
        if connected_hosts.len() > 1 {
            let mut host_selector = h_flex()
                .w_full()
                .px_2()
                .py_1()
                .gap_1()
                .border_b_1()
                .border_color(cx.theme().colors().border);

            for (host_idx, host_name) in connected_hosts.iter().enumerate() {
                let is_selected = self.selected_host.as_deref() == Some(host_name.as_str());
                let name = host_name.clone();
                host_selector = host_selector.child(
                    Button::new(("host-tab", host_idx), host_name.clone())
                        .label_size(LabelSize::XSmall)
                        .style(if is_selected {
                            ButtonStyle::Filled
                        } else {
                            ButtonStyle::Subtle
                        })
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.selected_host = Some(name.clone());
                            cx.notify();
                        })),
                );
            }
            panel = panel.child(host_selector);
        }

        if show_form {
            let local_port_display = if self.form_local_port.is_empty() {
                "local port".to_string()
            } else {
                self.form_local_port.clone()
            };

            let remote_host_display = if self.form_remote_host.is_empty() {
                "remote host (optional)".to_string()
            } else {
                self.form_remote_host.clone()
            };

            let remote_port_display = if self.form_remote_port.is_empty() {
                "remote port".to_string()
            } else {
                self.form_remote_port.clone()
            };

            let form = v_flex()
                .p_2()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new("New Port Forward")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            Label::new(format!("Local: {}", local_port_display))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format!("Host: {}", remote_host_display))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format!("Remote: {}", remote_port_display))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("submit-forward", "Forward")
                                .style(ButtonStyle::Filled)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.add_forward_from_form(cx);
                                })),
                        )
                        .child(
                            Button::new("cancel-forward", "Cancel")
                                .style(ButtonStyle::Subtle)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.show_add_form = false;
                                    cx.notify();
                                })),
                        ),
                );
            panel = panel.child(form);
        }

        // Filter forwards by selected host
        let selected_host = self.selected_host.clone();
        let visible_forwards: Vec<(usize, &PortForwardEntry)> = self
            .forwards
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                selected_host
                    .as_ref()
                    .map(|h| f.host_name == *h)
                    .unwrap_or(true)
            })
            .collect();

        if !visible_forwards.is_empty() {
            let mut rows = v_flex().id("forwards-list").flex_1().overflow_y_scroll();

            for (index, entry) in visible_forwards {
                let (status_color, status_text) = match &entry.status {
                    ForwardStatus::Active => (Color::Success, "Active".to_string()),
                    ForwardStatus::Inactive => (Color::Muted, "Inactive".to_string()),
                    ForwardStatus::Starting => (Color::Warning, "Starting...".to_string()),
                    ForwardStatus::Failed(reason) => {
                        (Color::Error, format!("Failed: {reason}"))
                    }
                };

                let remote_host = entry
                    .option
                    .remote_host
                    .as_deref()
                    .unwrap_or("localhost");

                let label = format!(
                    "localhost:{} \u{2192} {}:{}",
                    entry.option.local_port, remote_host, entry.option.remote_port
                );

                let is_active = entry.status == ForwardStatus::Active;
                let hover_bg = cx.theme().colors().ghost_element_hover;

                let mut row = h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .hover(move |style| style.bg(hover_bg))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_1()
                            .child(
                                div()
                                    .w(px(8.))
                                    .h(px(8.))
                                    .rounded_full()
                                    .bg(status_color.color(cx)),
                            )
                            .child(
                                v_flex()
                                    .child(Label::new(label).size(LabelSize::Small))
                                    .child(
                                        Label::new(status_text)
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            ),
                    );

                let buttons = h_flex().gap_1().when(is_active, |el| {
                    el.child(
                        IconButton::new(("stop-forward", index), IconName::Stop)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Stop Forward"))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.stop_forward(index, cx);
                            })),
                    )
                })
                .child(
                    IconButton::new(("remove-forward", index), IconName::Close)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Remove Forward"))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.remove_forward(index, cx);
                        })),
                );

                row = row.child(buttons);
                rows = rows.child(row);
            }

            panel = panel.child(rows);
        } else if has_forwards {
            panel = panel.child(
                div().p_4().child(
                    Label::new("No forwards for this host.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        } else {
            panel = panel.child(
                div().p_4().child(
                    Label::new("No port forwards configured. Click + to add one.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        }

        // Detected remote ports section
        if let Some(host) = &self.selected_host {
            if let Some(detected) = self.detected_ports.get(host) {
                if !detected.is_empty() {
                    // Filter out ports that already have a forward configured
                    let forwarded_ports: std::collections::HashSet<u16> = self
                        .forwards
                        .iter()
                        .filter(|f| f.host_name == *host)
                        .map(|f| f.option.remote_port)
                        .collect();

                    let unforwarded: Vec<&DetectedPort> = detected
                        .iter()
                        .filter(|d| !forwarded_ports.contains(&d.port))
                        .collect();

                    if !unforwarded.is_empty() {
                        let mut section = v_flex()
                            .border_t_1()
                            .border_color(cx.theme().colors().border)
                            .child(
                                h_flex()
                                    .px_2()
                                    .py_1()
                                    .child(
                                        Label::new("Detected Remote Ports")
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    ),
                            );

                        for dp in unforwarded {
                            let port = dp.port;
                            let label = match &dp.process {
                                Some(proc) => format!(":{} ({})", port, proc),
                                None => format!(":{}", port),
                            };
                            let host_name = host.clone();
                            let hover_bg = cx.theme().colors().ghost_element_hover;

                            section = section.child(
                                h_flex()
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .py_0p5()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(hover_bg))
                                    .child(
                                        Label::new(label)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        Button::new(("forward-detected", port as usize), "Forward")
                                            .label_size(LabelSize::XSmall)
                                            .style(ButtonStyle::Subtle)
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.add_detected_forward(
                                                        &host_name, port, cx,
                                                    );
                                                },
                                            )),
                                    ),
                            );
                        }

                        panel = panel.child(section);
                    }
                }
            }
        }

        panel.into_any_element()
    }
}
