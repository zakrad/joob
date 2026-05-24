#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use joob_core::config::{ClientConfig, Profile};
use joob_core::tunnel::ClientTunnel;
use std::sync::Arc;
use tokio::sync::watch;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
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
        Box::new(|cc| Ok(Box::new(JoobApp::new(cc)))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
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
}

impl JoobApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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

            match ClientTunnel::start(config).await {
                Ok(()) => {
                    let _ = tx.send(ConnectionState::Connected);
                }
                Err(e) => {
                    tracing::error!("Tunnel error: {}", e);
                    let _ = tx.send(ConnectionState::Disconnected);
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
            self.state = *self.state_rx.borrow_and_update();
            match self.state {
                ConnectionState::Connected => {
                    self.status_msg = String::from("Connected ✓");
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
                let (color, label) = match self.state {
                    ConnectionState::Disconnected => (egui::Color32::from_rgb(200, 60, 60), "● Disconnected"),
                    ConnectionState::Connecting => (egui::Color32::from_rgb(200, 180, 40), "◉ Connecting..."),
                    ConnectionState::Connected => (egui::Color32::from_rgb(40, 200, 80), "● Connected"),
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
                let is_connected = self.state != ConnectionState::Disconnected;
                let button_text = if is_connected { "Disconnect" } else { "Connect" };
                let button_color = if is_connected {
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

                let enabled = self.state != ConnectionState::Connecting;
                if ui.add_enabled(enabled, button).clicked() {
                    if is_connected {
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

            ui.label(
                egui::RichText::new(&self.status_msg)
                    .color(egui::Color32::GRAY)
                    .size(12.0),
            );
        });
    }
}
