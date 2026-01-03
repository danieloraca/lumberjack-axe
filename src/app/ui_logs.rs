use eframe::egui;

use crate::app::App;
use crate::app::state::{Theme, format_timestamp_millis, try_pretty_json};

pub fn draw_logs_view(app: &mut App, ui: &mut egui::Ui) {
    ui.label("Logs (CloudWatch via AWS SDK):");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Filter (CloudWatch pattern):");
        let filter_response =
            ui.add(egui::TextEdit::singleline(&mut app.logs_view.filter_text).desired_width(250.0));

        if filter_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.start_fetch_logs(std::time::Duration::from_secs(5 * 60));
        }

        ui.separator();

        ui.checkbox(&mut app.logs_view.tail_mode, "Tail");

        ui.separator();

        ui.checkbox(&mut app.logs_view.show_local_time, "Local time");

        ui.separator();
        ui.label("Tail every (s):");
        let mut interval = app.logs_view.tail_interval_secs as i32;
        if ui
            .add(egui::DragValue::new(&mut interval).range(1..=300))
            .changed()
        {
            app.logs_view.tail_interval_secs = interval.max(1) as u64;
        }
    });

    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for entry in app.logs_view.entries.iter() {
                let ts_formatted =
                    format_timestamp_millis(entry.timestamp_millis, app.logs_view.show_local_time);

                if !app.logs_view.filter_text.is_empty()
                    && !entry
                        .message
                        .to_lowercase()
                        .contains(&app.logs_view.filter_text.to_lowercase())
                {
                    continue;
                }

                let level_color = if app.theme == Theme::RetroGreen {
                    if entry.message.contains("ERROR") {
                        egui::Color32::from_rgb(0xff, 0x40, 0x40)
                    } else if entry.message.contains("WARN") {
                        egui::Color32::from_rgb(0xff, 0xff, 0x80)
                    } else {
                        egui::Color32::from_rgb(0x00, 0xff, 0x66)
                    }
                } else {
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

                if let Some(pretty_json) = try_pretty_json(&entry.message) {
                    let mut s = pretty_json.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut s)
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
