use eframe::egui;

use crate::app::App;

pub fn draw_status_bar(app: &App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                use egui::Align;

                let status = if app.is_fetching {
                    "Fetching logs…".to_string()
                } else if app.is_loading_groups {
                    "Loading log groups…".to_string()
                } else if let Some(err) = &app.last_error {
                    if err.len() > 120 {
                        format!("Error: {}…", &err[..117])
                    } else {
                        format!("Error: {err}")
                    }
                } else {
                    "Ready".to_string()
                };

                if app.last_error.is_some() && !app.is_fetching && !app.is_loading_groups {
                    ui.colored_label(egui::Color32::RED, status);
                } else {
                    ui.label(status);
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(format!(
                        "Tail: {}",
                        if app.logs_view.tail_mode { "ON" } else { "OFF" }
                    ));
                });
            });
        });
}
