#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use joob_core::config::{ClientConfig, Profile};
use joob_core::tunnel::ClientTunnel;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

/// Ring buffer for GUI-visible log messages.
const MAX_LOG_LINES: usize = 100;
type LogBuffer = Arc<Mutex<VecDeque<String>>>;

/// A tracing layer that captures WARN and ERROR messages into a shared buffer.
struct GuiLogLayer {
    buffer: LogBuffer,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GuiLogLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use tracing::Level;
        let meta = event.metadata();
        if *meta.level() > Level::WARN {
            return; // only capture WARN and ERROR
        }

        // Extract the message
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let line = format!("[{}] {}", meta.level(), visitor.0);
        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= MAX_LOG_LINES {
                buf.pop_front();
            }
            buf.push_back(line);
        }
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        } else if !self.0.is_empty() {
            self.0 += &format!(" {}={:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        } else if !self.0.is_empty() {
            self.0 += &format!(" {}={}", field.name(), value);
        }
    }
}

fn main() -> eframe::Result<()> {
    let log_buffer: LogBuffer = Arc::new(Mutex::new(VecDeque::new()));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        );

    let gui_layer = GuiLogLayer {
        buffer: Arc::clone(&log_buffer),
    };

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(gui_layer)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Joob")
            .with_inner_size([440.0, 520.0])
            .with_min_inner_size([400.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Joob",
        options,
        Box::new(move |cc| Ok(Box::new(JoobApp::new(cc, log_buffer)))),
    )
}

#[derive(PartialEq, Clone)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

struct JoobApp {
    profile_input: String,
    config: Option<ClientConfig>,
    state: ConnectionState,
    status_msg: String,
    socks_port: String,
    http_port: String,
    // Channel to receive state updates from the tunnel task
    state_rx: watch::Receiver<ConnectionState>,
    state_tx: watch::Sender<ConnectionState>,
    // Tokio runtime for background tunnel
    runtime: Arc<tokio::runtime::Runtime>,
    // Handle to abort the tunnel
    tunnel_handle: Option<tokio::task::JoinHandle<()>>,
    // Log buffer for GUI display
    log_buffer: LogBuffer,
    show_logs: bool,
}

impl JoobApp {
    fn new(_cc: &eframe::CreationContext<'_>, log_buffer: LogBuffer) -> Self {
        let (state_tx, state_rx) = watch::channel(ConnectionState::Disconnected);

        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime"),
        );

        // Try to load saved config
        let (config, profile_input) = if std::path::Path::new("client.json").exists() {
            match ClientConfig::load_from_file("client.json") {
                Ok(cfg) => (Some(cfg), String::from("[loaded from client.json]")),
                Err(_) => (None, String::new()),
            }
        } else {
            (None, String::new())
        };

        Self {
            profile_input,
            config,
            state: ConnectionState::Disconnected,
            status_msg: String::from("Ready"),
            socks_port: String::from("1080"),
            http_port: String::from("8080"),
            state_rx,
            state_tx,
            runtime,
            tunnel_handle: None,
            log_buffer,
            show_logs: false,
        }
    }

    fn import_profile(&mut self) {
        let input = self.profile_input.trim();
        if input.is_empty() || input == "[loaded from client.json]" {
            return;
        }

        match Profile::decode(input) {
            Ok(cfg) => {
                // Save to file
                if let Err(e) = cfg.save_to_file("client.json") {
                    self.status_msg = format!("Save error: {}", e);
                    return;
                }
                self.socks_port = cfg.socks_port.to_string();
                self.http_port = cfg.http_port.to_string();
                self.config = Some(cfg);
                self.status_msg = String::from("Profile imported ✓");
            }
            Err(e) => {
                self.status_msg = format!("Invalid profile: {}", e);
            }
        }
    }

    fn connect(&mut self) {
        let Some(mut config) = self.config.clone() else {
            self.status_msg = String::from("No profile loaded");
            return;
        };

        // Apply port overrides
        if let Ok(p) = self.socks_port.parse::<u16>() {
            config.socks_port = p;
        }
        if let Ok(p) = self.http_port.parse::<u16>() {
            config.http_port = p;
        }

        self.state = ConnectionState::Connecting;
        self.status_msg = String::from("Connecting...");

        let tx = self.state_tx.clone();
        let handle = self.runtime.spawn(async move {
            let _ = tx.send(ConnectionState::Connecting);

            let tx_ready = tx.clone();
            let on_ready = move || {
                let _ = tx_ready.send(ConnectionState::Connected);
            };

            match ClientTunnel::start_with_callback(config, Some(on_ready)).await {
                Ok(()) => {
                    // Normal shutdown (ctrl+c or abort)
                    let _ = tx.send(ConnectionState::Disconnected);
                }
                Err(e) => {
                    tracing::error!("Tunnel error: {}", e);
                    let _ = tx.send(ConnectionState::Failed(e.to_string()));
                }
            }
        });

        self.tunnel_handle = Some(handle);
    }

    fn disconnect(&mut self) {
        if let Some(handle) = self.tunnel_handle.take() {
            handle.abort();
        }
        self.state = ConnectionState::Disconnected;
        self.status_msg = String::from("Disconnected");
        let _ = self.state_tx.send(ConnectionState::Disconnected);
    }

    fn paste_from_clipboard(&mut self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                self.profile_input = text;
            }
        }
    }
}

impl eframe::App for JoobApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for state updates from tunnel task
        if self.state_rx.has_changed().unwrap_or(false) {
            let new_state = self.state_rx.borrow_and_update().clone();
            match &new_state {
                ConnectionState::Connected => {
                    self.status_msg = String::from("Connected ✓");
                }
                ConnectionState::Failed(err) => {
                    self.status_msg = format!("Error: {}", err);
                }
                ConnectionState::Disconnected => {
                    if self.status_msg == "Connecting..." {
                        self.status_msg = String::from("Connection failed");
                    }
                }
                ConnectionState::Connecting => {
                    self.status_msg = String::from("Connecting...");
                }
            }
            self.state = new_state;
        }

        // Request repaint periodically to check state updates
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);

                // Title
                ui.heading(egui::RichText::new("جوب  —  Joob").size(28.0));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Tunnel over Google Drive")
                        .color(egui::Color32::GRAY)
                        .size(14.0),
                );
                ui.add_space(20.0);

                // Status indicator
                let (color, label) = match &self.state {
                    ConnectionState::Disconnected => (egui::Color32::from_rgb(200, 60, 60), "● Disconnected".to_string()),
                    ConnectionState::Connecting => (egui::Color32::from_rgb(200, 180, 40), "◉ Connecting...".to_string()),
                    ConnectionState::Connected => (egui::Color32::from_rgb(40, 200, 80), "● Connected".to_string()),
                    ConnectionState::Failed(_) => (egui::Color32::from_rgb(200, 60, 60), "● Error".to_string()),
                };
                ui.label(egui::RichText::new(label).color(color).size(18.0));
                ui.add_space(16.0);
            });

            ui.separator();
            ui.add_space(8.0);

            // Profile section
            ui.group(|ui| {
                ui.label(egui::RichText::new("Profile").strong());
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.profile_input)
                            .hint_text("Paste joob:// profile string")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    if ui.button("📋 Paste").clicked() {
                        self.paste_from_clipboard();
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.import_profile();
                    }
                });

                ui.add_space(4.0);
                if ui.button("Import Profile").clicked() {
                    self.import_profile();
                }
            });

            ui.add_space(8.0);

            // Port settings
            ui.group(|ui| {
                ui.label(egui::RichText::new("Proxy Settings").strong());
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("SOCKS5 Port:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.socks_port)
                            .desired_width(80.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("HTTP Port:    ");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.http_port)
                            .desired_width(80.0),
                    );
                });
            });

            ui.add_space(12.0);

            // Connect / Disconnect button
            ui.vertical_centered(|ui| {
                let is_active = matches!(self.state, ConnectionState::Connecting | ConnectionState::Connected);
                let button_text = if is_active { "Disconnect" } else { "Connect" };
                let button_color = if is_active {
                    egui::Color32::from_rgb(200, 60, 60)
                } else {
                    egui::Color32::from_rgb(40, 140, 200)
                };

                let button = egui::Button::new(
                    egui::RichText::new(button_text)
                        .size(18.0)
                        .color(egui::Color32::WHITE),
                )
                .min_size(egui::vec2(200.0, 44.0))
                .fill(button_color)
                .corner_radius(8.0);

                let enabled = !matches!(self.state, ConnectionState::Connecting);
                if ui.add_enabled(enabled, button).clicked() {
                    if is_active {
                        self.disconnect();
                    } else {
                        self.connect();
                    }
                }
            });

            ui.add_space(12.0);

            // Status bar
            ui.separator();
            ui.add_space(4.0);

            if self.state == ConnectionState::Connected {
                ui.horizontal(|ui| {
                    ui.label("SOCKS5:");
                    let addr = format!("127.0.0.1:{}", self.socks_port);
                    if ui.link(&addr).clicked() {
                        let mut clipboard = arboard::Clipboard::new().ok();
                        if let Some(ref mut cb) = clipboard {
                            let _ = cb.set_text(&addr);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("HTTP:");
                    let addr = format!("127.0.0.1:{}", self.http_port);
                    if ui.link(&addr).clicked() {
                        let mut clipboard = arboard::Clipboard::new().ok();
                        if let Some(ref mut cb) = clipboard {
                            let _ = cb.set_text(&addr);
                        }
                    }
                });
                ui.add_space(4.0);
            }

            let msg_color = if self.status_msg.starts_with("Error:") {
                egui::Color32::from_rgb(255, 100, 100)
            } else {
                egui::Color32::GRAY
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.status_msg)
                        .color(msg_color)
                        .size(12.0),
                )
                .wrap(),
            );

            ui.add_space(8.0);

            // Log panel toggle
            if ui
                .selectable_label(self.show_logs, "Show Logs")
                .clicked()
            {
                self.show_logs = !self.show_logs;
            }

            if self.show_logs {
                ui.separator();
                let logs = self
                    .log_buffer
                    .lock()
                    .map(|buf| buf.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();

                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &logs {
                            let color = if line.starts_with("[ERROR]") {
                                egui::Color32::from_rgb(255, 100, 100)
                            } else {
                                egui::Color32::from_rgb(255, 200, 60)
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(line).color(color).size(11.0).monospace(),
                                )
                                .wrap(),
                            );
                        }
                        if logs.is_empty() {
                            ui.label(
                                egui::RichText::new("No warnings or errors")
                                    .color(egui::Color32::DARK_GRAY)
                                    .size(11.0),
                            );
                        }
                    });
            }
        });
    }
}
