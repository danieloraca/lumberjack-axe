use std::time::{Duration, Instant};

use eframe::egui;

use crate::aws::{AwsLogError, LogEntry};
use crate::worker::{WorkerHandle, WorkerRequest};

pub mod state;
pub mod status_bar;
pub mod ui_logs;

use state::{ActiveView, LogsViewState, Theme};

pub struct App {
    pub(crate) view: ActiveView,
    pub(crate) logs_view: LogsViewState,
    pub(crate) should_close: bool,
    pub(crate) last_error: Option<String>,
    pub(crate) is_fetching: bool,
    pub(crate) fetch_rx: Option<std::sync::mpsc::Receiver<Result<Vec<LogEntry>, AwsLogError>>>,
    pub(crate) groups_rx: Option<std::sync::mpsc::Receiver<Result<Vec<String>, AwsLogError>>>,
    pub(crate) worker: WorkerHandle,
    pub(crate) theme: Theme,
    pub(crate) is_loading_groups: bool,
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

    fn start_fetch_logs(&mut self, lookback: Duration) {
        if self.is_fetching {
            return;
        }

        let profile = self.logs_view.profile.clone();
        let region = self.logs_view.region.clone();
        let mut log_group = self.logs_view.log_group.clone();
        let filter = self.logs_view.filter_text.clone();

        log_group = log_group.trim().to_string();
        if log_group.is_empty() {
            self.last_error = Some("Please select a log group.".to_string());
            return;
        }
        self.logs_view.log_group = log_group.clone();

        self.is_fetching = true;
        self.last_error = None;

        let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<LogEntry>, AwsLogError>>();

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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.should_close {
            // hook for future close behavior
        }

        // Apply theme visuals.
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

        // Poll fetch results.
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
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.is_fetching = false;
                    self.fetch_rx = None;
                }
            }
        }

        // Poll group list results.
        if let Some(rx) = self.groups_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(groups)) => {
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
                    self.last_error = Some(format!("{err}"));
                    self.groups_rx = None;
                    self.is_loading_groups = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.groups_rx = None;
                    self.is_loading_groups = false;
                }
            }
        }

        // Tail logic.
        if self.logs_view.tail_mode && !self.is_fetching {
            let now = Instant::now();
            let should_trigger = match self.logs_view.last_tail_instant {
                Some(last) => {
                    now.duration_since(last).as_secs() >= self.logs_view.tail_interval_secs
                }
                None => true,
            };

            if should_trigger {
                self.start_fetch_logs(Duration::from_secs(5 * 60));
                self.logs_view.last_tail_instant = Some(now);
            }
        } else if !self.logs_view.tail_mode {
            self.logs_view.last_tail_instant = None;
        }

        // Top bar.
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            // First row: title + view + theme + version/close
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

            // Second row: AWS settings.
            ui.horizontal(|ui| {
                ui.label("Profile:");
                ui.add(egui::TextEdit::singleline(&mut self.logs_view.profile).desired_width(80.0));

                ui.separator();

                ui.label("Region:");
                ui.add(egui::TextEdit::singleline(&mut self.logs_view.region).desired_width(100.0));

                ui.separator();

                let load_btn =
                    ui.add_enabled(!self.is_loading_groups, egui::Button::new("Load groups"));
                if load_btn.clicked() {
                    self.start_load_log_groups();
                }

                if self.is_loading_groups {
                    ui.spinner();
                }
            });

            // Third row: group + fetch.
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

        // Main content.
        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            ActiveView::Logs => ui_logs::draw_logs_view(self, ui),
        });

        // Status bar.
        status_bar::draw_status_bar(self, ctx);
    }
}
