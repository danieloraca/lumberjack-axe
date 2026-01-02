use crate::aws::{AwsLogError, FetchLogsParams, LogEntry, fetch_recent_logs};
use crate::worker::{WorkerHandle, WorkerRequest};
use chrono::{DateTime, Local, Utc};
use eframe::egui;
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
        }
    }

    /// Notify the app that the tray requested the window to close/hide.
    ///
    /// The actual tray integration will live in another module, but it can call
    /// this method (through a handle or message channel) to drive the UI.
    pub fn request_close(&mut self) {
        self.should_close = true;
    }

    /// Whether the app has requested the surrounding frame/window to close.
    ///
    /// The `eframe::App` implementation can use this to tell the frame to close.
    pub fn should_close(&self) -> bool {
        self.should_close
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Honor external close requests, e.g. from tray/menu integration.
        if self.should_close {
            //
        }

        match self.theme {
            Theme::Light => ctx.set_visuals(egui::Visuals::light()),
            Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            Theme::RetroGreen => {
                let mut visuals = egui::Visuals::dark();
                // Retro CRT style
                visuals.override_text_color = Some(egui::Color32::from_rgb(0x00, 0xff, 0x66));
                visuals.panel_fill = egui::Color32::BLACK;
                visuals.extreme_bg_color = egui::Color32::BLACK;
                visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x00, 0x20, 0x00);
                visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x00, 0x40, 0x00);
                visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x00, 0x60, 0x00);
                ctx.set_visuals(visuals);
            }
        }

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
                }
                Ok(Err(err)) => {
                    eprintln!("[axe] groups_rx received ERROR: {err}");
                    self.last_error = Some(format!("{err}"));
                    self.groups_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still loading; nothing to do this frame.
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    eprintln!("[axe] groups_rx disconnected");
                    self.groups_rx = None;
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
                ui.text_edit_singleline(&mut self.logs_view.profile);

                ui.separator();

                ui.label("Region:");
                ui.text_edit_singleline(&mut self.logs_view.region);

                ui.separator();

                if ui.button("Load groups").clicked() {
                    self.start_load_log_groups();
                }
            });

            // Third row: log group selection + manual override + fetch
            ui.horizontal(|ui| {
                // Optional dropdown if we have groups.
                if !self.logs_view.available_groups.is_empty() {
                    let current_group_name = self
                        .logs_view
                        .selected_group_index
                        .and_then(|idx| self.logs_view.available_groups.get(idx))
                        .cloned()
                        .unwrap_or_else(|| self.logs_view.log_group.clone());

                    egui::ComboBox::from_label("Log group:")
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
                }

                ui.separator();

                // Always-available manual log group input.
                ui.label("Log group (manual):");
                ui.text_edit_singleline(&mut self.logs_view.log_group);

                ui.separator();

                if ui
                    .add_enabled(!self.is_fetching, egui::Button::new("Fetch last 5m"))
                    .clicked()
                {
                    self.start_fetch_logs(Duration::from_secs(5 * 60));
                }
            });
        });

        // Main content: delegate to the active view.
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            ActiveView::Logs => self.render_logs_view(ui),
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
        ui.label("Logs (CloudWatch via AWS SDK):");
        ui.separator();

        if let Some(err) = &self.last_error {
            ui.colored_label(egui::Color32::RED, err);
            ui.separator();
        }

        // Controls row: filter and tail toggle.
        ui.horizontal(|ui| {
            ui.label("Filter (CloudWatch pattern):");
            let filter_response = ui.text_edit_singleline(&mut self.logs_view.filter_text);

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
                .add(egui::DragValue::new(&mut interval).clamp_range(1..=300))
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
                    ui.label(egui::RichText::new(&entry.message).color(level_color));
                    ui.separator();
                }
            });
    }
}

fn format_timestamp_millis(ts_millis: i64, use_local: bool) -> String {
    if ts_millis <= 0 {
        return "-".to_string();
    }

    let secs = ts_millis / 1000;
    let nanos = (ts_millis % 1000) * 1_000_000;
    let naive = match chrono::NaiveDateTime::from_timestamp_opt(secs, nanos as u32) {
        Some(n) => n,
        None => return "-".to_string(),
    };

    if use_local {
        let dt: DateTime<Local> = DateTime::from_utc(naive, *Local::now().offset());
        dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
    } else {
        let dt: DateTime<Utc> = DateTime::<Utc>::from_utc(naive, Utc);
        dt.format("%Y-%m-%d %H:%M:%S%.3fZ").to_string()
    }
}
