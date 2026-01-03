use eframe::egui;

use crate::app::App;

pub fn draw_status_bar(app: &App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                use egui::Align;

                let (status, is_error) = compute_status(app);

                if is_error {
                    ui.colored_label(egui::Color32::RED, status);
                } else {
                    ui.label(status);
                }

                // Friendly hint for common AWS auth issues.
                if let Some(err) = &app.last_error {
                    if err.contains("ExpiredTokenException")
                        || err.contains("The security token included in the request is expired")
                    {
                        ui.label(
                            egui::RichText::new(
                                "Hint: AWS credentials expired. Re-run your AWS login.",
                            )
                            .italics(),
                        );
                    }
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

fn compute_status(app: &App) -> (String, bool) {
    if app.is_fetching {
        ("Fetching logs…".to_string(), false)
    } else if app.is_loading_groups {
        ("Loading log groups…".to_string(), false)
    } else if let Some(err) = &app.last_error {
        let msg = if err.len() > 61 {
            format!("Error: {}…", &err[..58])
        } else {
            format!("Error: {err}")
        };
        (msg, true)
    } else if let Some(info) = &app.last_info {
        (info.clone(), false) // <--- show info when present
    } else {
        ("Ready".to_string(), false)
    }
}
