use crate::aws::{AwsLogError, LogEntry};
use crate::worker::{WorkerHandle, WorkerRequest};
use chrono::{Local, TimeZone, Utc};
use eframe::egui;
use serde_json::Value as JsonValue;
use std::time::Duration;
use std::time::Instant;

pub struct App {
    view: ActiveView,
    logs_view: LogsViewState,
    should_close: bool,
    last_error: Option<String>,
    is_fetching: bool,

    fetch_rx: Option<std::sync::mpsc::Receiver<Result<Vec<LogEntry>, AwsLogError>>>,
    groups_rx: Option<std::sync::mpsc::Receiver<Result<Vec<String>, AwsLogError>>>,

    /// Handle to the background AWS worker.
    worker: WorkerHandle,
    theme: Theme,
    is_loading_groups: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Logs,
    // Settings,
    // Favorites,
}

#[derive(Default)]
pub struct LogsViewState {
    pub profile: String,
    pub region: String,
    pub log_group: String,
    pub filter_text: String,
    pub available_groups: Vec<String>,
    pub selected_group_index: Option<usize>,
    pub tail_mode: bool,
    pub show_local_time: bool,
    pub entries: Vec<LogEntry>,
    pub tail_interval_secs: u64,
    pub last_tail_instant: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    RetroGreen,
}

impl LogsViewState {
    pub fn new_default() -> Self {
        Self {
            profile: "form".to_string(),
            region: "eu-west-1".to_string(),
            log_group: String::new(),
            filter_text: String::new(),
            tail_mode: false,
            show_local_time: false,
            entries: Vec::new(),
            available_groups: Vec::new(),
            selected_group_index: None,
            tail_interval_secs: 5,
            last_tail_instant: None,
        }
    }
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>, worker: WorkerHandle) -> Self {
        Self {
            view: ActiveView::Logs,
            logs_view: LogsViewState::new_default(),
            should_close: false,
            last_error: None,
            is_fetching: false,
            fetch_rx: None,
            groups_rx: None,
            worker,
            theme: Theme::Dark,
            is_loading_groups: false,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.should_close {
            //
        }

        match self.theme {
            Theme::Light => ctx.set_visuals(egui::Visuals::light()),
            Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            Theme::RetroGreen => {
                let mut visuals = egui::Visuals::dark();
                visuals.override_text_color = Some(egui::Color32::from_rgb(0x00, 0xff, 0x66));
                visuals.panel_fill = egui::Color32::BLACK;
                visuals.extreme_bg_color = egui::Color32::BLACK;
                visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x00, 0x20, 0x00);
                visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x00, 0x40, 0x00);
                visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x00, 0x60, 0x00);
                ctx.set_visuals(visuals);
            }
        }

        // Apply larger global fonts
        let mut style: egui::Style = (*ctx.style()).clone();
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(26.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(17.0, egui::FontFamily::Monospace),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Small,
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();
        ctx.set_style(style);

        // Check for results from any in-flight background fetch.
        if let Some(rx) = self.fetch_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(entries)) => {
                    self.logs_view.entries = entries;
                    self.is_fetching = false;
                    self.fetch_rx = None;
                }
                Ok(Err(err)) => {
                    self.last_error = Some(format!("{err}"));
                    self.is_fetching = false;
                    self.fetch_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still fetching; nothing to do this frame.
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Background task died; stop waiting.
                    self.is_fetching = false;
                    self.fetch_rx = None;
                }
            }
        }

        // Check for results from any in-flight log group list load.
        // Check for results from any in-flight log group list load.
        if let Some(rx) = self.groups_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(groups)) => {
                    println!("[axe] groups_rx received OK: {} groups", groups.len());
                    self.logs_view.available_groups = groups;
                    if let Some(idx) = self.logs_view.selected_group_index {
                        if idx >= self.logs_view.available_groups.len() {
                            self.logs_view.selected_group_index = None;
                        }
                    }
                    self.groups_rx = None;
                    self.is_loading_groups = false;
                }
                Ok(Err(err)) => {
                    eprintln!("[axe] groups_rx received ERROR: {err}");
                    self.last_error = Some(format!("{err}"));
                    self.groups_rx = None;
                    self.is_loading_groups = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still loading; nothing to do this frame.
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    eprintln!("[axe] groups_rx disconnected");
                    self.groups_rx = None;
                    self.is_loading_groups = false;
                }
            }
        }

        if self.logs_view.tail_mode && !self.is_fetching {
            let now = Instant::now();
            let should_trigger = match self.logs_view.last_tail_instant {
                Some(last) => {
                    now.duration_since(last).as_secs() >= self.logs_view.tail_interval_secs
                }
                None => true, // first time
            };

            if should_trigger {
                // Use the same lookback as the manual "Fetch last 5m".
                self.start_fetch_logs(Duration::from_secs(5 * 60));
                self.logs_view.last_tail_instant = Some(now);
            }
        } else if !self.logs_view.tail_mode {
            // If tail is off, clear the last_tail_instant so it restarts immediately
            // next time it's turned on.
            self.logs_view.last_tail_instant = None;
        }

        // Top navigation bar (view selection, basic actions).
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            // First row: title + view + version/close
            ui.horizontal(|ui| {
                ui.heading("Lumberjack Axe");

                ui.separator();

                ui.selectable_value(&mut self.view, ActiveView::Logs, "Logs");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕").clicked() {
                        self.should_close = true;
                    }
                    ui.label("v0.1.0");

                    ui.separator();

                    // Theme toggle button
                    let theme_label = match self.theme {
                        Theme::Light => "Theme: Light",
                        Theme::Dark => "Theme: Dark",
                        Theme::RetroGreen => "Theme: Retro",
                    };
                    if ui.button(theme_label).clicked() {
                        self.theme = match self.theme {
                            Theme::Light => Theme::Dark,
                            Theme::Dark => Theme::RetroGreen,
                            Theme::RetroGreen => Theme::Light,
                        };
                    }
                });
            });

            ui.separator();

            // Second row: AWS connection settings (Profile, Region, Load groups)
            ui.horizontal(|ui| {
                ui.label("Profile:");
                ui.add(egui::TextEdit::singleline(&mut self.logs_view.profile).desired_width(80.0));

                ui.separator();

                ui.label("Region:");
                ui.add(egui::TextEdit::singleline(&mut self.logs_view.region).desired_width(100.0));

                ui.separator();

                // Disable the button while a load is in progress
                let load_btn =
                    ui.add_enabled(!self.is_loading_groups, egui::Button::new("Load groups"));
                if load_btn.clicked() {
                    self.start_load_log_groups();
                }

                // Visual indicator
                if self.is_loading_groups {
                    ui.spinner(); // built-in egui spinner
                    ui.label("Loading...");
                }
            });

            // Third row: log group selection + manual override + fetch
            ui.horizontal(|ui| {
                ui.label("Group:");
                let current_group_name = self
                    .logs_view
                    .selected_group_index
                    .and_then(|idx| self.logs_view.available_groups.get(idx))
                    .cloned()
                    .unwrap_or_else(|| self.logs_view.log_group.clone());

                egui::ComboBox::from_id_salt("log_group_combo")
                    .selected_text(if current_group_name.is_empty() {
                        "<none>"
                    } else {
                        current_group_name.as_str()
                    })
                    .show_ui(ui, |ui| {
                        for (idx, name) in self.logs_view.available_groups.iter().enumerate() {
                            let selected = Some(idx) == self.logs_view.selected_group_index;
                            if ui.selectable_label(selected, name).clicked() {
                                self.logs_view.selected_group_index = Some(idx);
                                self.logs_view.log_group = name.clone();
                            }
                        }
                    });

                ui.separator();

                let fetch_btn =
                    ui.add_enabled(!self.is_fetching, egui::Button::new("Fetch last 5m"));
                if fetch_btn.clicked() {
                    self.start_fetch_logs(Duration::from_secs(5 * 60));
                }

                if self.is_fetching {
                    ui.spinner();
                }
            });
        });

        // Main content: delegate to the active view.
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            ActiveView::Logs => self.render_logs_view(ui),
        });

        // Status bar at the bottom
        egui::TopBottomPanel::bottom("status_bar")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    use egui::Align;

                    // Left: status text
                    let status = if self.is_fetching {
                        "Fetching logs…".to_string()
                    } else if self.is_loading_groups {
                        "Loading log groups…".to_string()
                    } else if let Some(err) = &self.last_error {
                        if err.len() > 120 {
                            format!("Error: {}…", &err[..117])
                        } else {
                            format!("Error: {err}")
                        }
                    } else {
                        "Ready".to_string()
                    };

                    if self.last_error.is_some() && !self.is_fetching && !self.is_loading_groups {
                        ui.colored_label(egui::Color32::RED, status);
                    } else {
                        ui.label(status);
                    }

                    // Right: tail status or other small info
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(format!(
                            "Tail: {}",
                            if self.logs_view.tail_mode {
                                "ON"
                            } else {
                                "OFF"
                            }
                        ));
                    });
                });
            });
    }
}

impl App {
    fn start_fetch_logs(&mut self, lookback: Duration) {
        if self.is_fetching {
            return;
        }

        let profile = self.logs_view.profile.clone();
        let region = self.logs_view.region.clone();
        let mut log_group = self.logs_view.log_group.clone();
        let filter = self.logs_view.filter_text.clone();

        // Trim whitespace from the log group name.
        log_group = log_group.trim().to_string();
        if log_group.is_empty() {
            self.last_error = Some("Please enter a log group name.".to_string());
            return;
        }
        // Persist the trimmed name back into state for UI.
        self.logs_view.log_group = log_group.clone();

        self.is_fetching = true;
        self.last_error = None;

        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<LogEntry>, AwsLogError>>();

        // Send request to the worker.
        self.worker.send(WorkerRequest::FetchRecentLogs {
            profile: if profile.trim().is_empty() {
                None
            } else {
                Some(profile)
            },
            region: if region.trim().is_empty() {
                None
            } else {
                Some(region)
            },
            log_group,
            filter_pattern: if filter.trim().is_empty() {
                None
            } else {
                Some(filter)
            },
            lookback,
            limit: 1_000,
            respond_to: tx,
        });

        self.fetch_rx = Some(rx);
    }

    fn start_load_log_groups(&mut self) {
        let profile = self.logs_view.profile.clone();
        let region = self.logs_view.region.clone();

        self.logs_view.available_groups.clear();
        self.logs_view.selected_group_index = None;
        self.last_error = None;

        self.is_loading_groups = true;

        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<String>, AwsLogError>>();

        self.worker.send(WorkerRequest::ListLogGroups {
            profile: if profile.trim().is_empty() {
                None
            } else {
                Some(profile)
            },
            region: if region.trim().is_empty() {
                None
            } else {
                Some(region)
            },
            limit: 50,
            respond_to: tx,
        });

        self.groups_rx = Some(rx);
    }

    /// Render the logs view (now backed by AWS).
    fn render_logs_view(&mut self, ui: &mut egui::Ui) {
        // Controls row: filter and tail toggle.
        ui.horizontal(|ui| {
            ui.label("Filter (CloudWatch pattern):");
            let filter_response = ui.add(
                egui::TextEdit::singleline(&mut self.logs_view.filter_text).desired_width(150.0), // pick whatever feels right
            );

            if filter_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.start_fetch_logs(Duration::from_secs(5 * 60));
            }

            ui.separator();

            ui.checkbox(&mut self.logs_view.tail_mode, "Tail");

            ui.separator();

            ui.checkbox(&mut self.logs_view.show_local_time, "Local time");

            ui.separator();
            ui.label("Tail every (s):");
            let mut interval = self.logs_view.tail_interval_secs as i32;
            if ui
                .add(egui::DragValue::new(&mut interval).range(1..=300))
                .changed()
            {
                self.logs_view.tail_interval_secs = interval.max(1) as u64;
            }
        });

        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for entry in self.logs_view.entries.iter() {
                    let ts_formatted = format_timestamp_millis(
                        entry.timestamp_millis,
                        self.logs_view.show_local_time,
                    );

                    if !self.logs_view.filter_text.is_empty()
                        && !entry
                            .message
                            .to_lowercase()
                            .contains(&self.logs_view.filter_text.to_lowercase())
                    {
                        continue;
                    }

                    let level_color = if self.theme == Theme::RetroGreen {
                        // retro palette
                        if entry.message.contains("ERROR") {
                            egui::Color32::from_rgb(0xff, 0x40, 0x40)
                        } else if entry.message.contains("WARN") {
                            egui::Color32::from_rgb(0xff, 0xff, 0x80)
                        } else {
                            egui::Color32::from_rgb(0x00, 0xff, 0x66)
                        }
                    } else {
                        // normal palette
                        if entry.message.contains("ERROR") {
                            egui::Color32::RED
                        } else if entry.message.contains("WARN") {
                            egui::Color32::YELLOW
                        } else if entry.message.contains("INFO") {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::WHITE
                        }
                    };

                    let header = match &entry.log_stream_name {
                        Some(stream) => format!("[{}] ({})", ts_formatted, stream),
                        None => format!("[{}]", ts_formatted),
                    };

                    ui.colored_label(egui::Color32::LIGHT_BLUE, header);

                    // Try JSON pretty-print; fall back to raw message.
                    if let Some(pretty_json) = try_pretty_json(&entry.message) {
                        // Render multi-line JSON in monospace with the level color.
                        ui.add(
                            egui::TextEdit::multiline(&mut pretty_json.clone())
                                .font(egui::TextStyle::Monospace)
                                .text_color(level_color)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    } else {
                        ui.label(egui::RichText::new(&entry.message).color(level_color));
                    }

                    ui.separator();
                }
            });
    }
}

fn format_timestamp_millis(ts_millis: i64, use_local: bool) -> String {
    use chrono::LocalResult;

    if ts_millis <= 0 {
        return "-".to_string();
    }

    let secs = ts_millis / 1000;
    let nanos = (ts_millis % 1000) * 1_000_000;

    if use_local {
        match Local.timestamp_opt(secs, nanos as u32) {
            LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            _ => "-".to_string(),
        }
    } else {
        match Utc.timestamp_opt(secs, nanos as u32) {
            LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S%.3fZ").to_string(),
            _ => "-".to_string(),
        }
    }
}

fn try_pretty_json(message: &str) -> Option<String> {
    // Quick heuristic: must start with { or [ and end with } or ] (after trimming).
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.as_bytes()[0] as char;
    let last = trimmed.as_bytes()[trimmed.len() - 1] as char;
    if !((first == '{' && last == '}') || (first == '[' && last == ']')) {
        return None;
    }

    // Try to parse as JSON.
    match serde_json::from_str::<JsonValue>(trimmed) {
        Ok(v) => {
            // Pretty-print with 2-space indentation.
            match serde_json::to_string_pretty(&v) {
                Ok(pretty) => Some(pretty),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}
