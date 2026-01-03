use crate::app::App;
use crate::app::state::{ActiveView, Theme};
use eframe::egui;

pub fn draw_top_bar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        // First row: title + view + theme + version/close
        ui.horizontal(|ui| {
            ui.heading("Lumberjack Axe");

            ui.separator();

            ui.selectable_value(&mut app.view, ActiveView::Logs, "Logs");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    app.should_close = true;
                }
                ui.label("v0.1.0");

                ui.separator();

                let theme_label = match app.theme {
                    Theme::Light => "Theme: Light",
                    Theme::Dark => "Theme: Dark",
                    Theme::RetroGreen => "Theme: Retro",
                };
                if ui.button(theme_label).clicked() {
                    app.theme = match app.theme {
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
            ui.add(egui::TextEdit::singleline(&mut app.logs_view.profile).desired_width(80.0));

            ui.separator();

            ui.label("Region:");
            ui.add(egui::TextEdit::singleline(&mut app.logs_view.region).desired_width(100.0));

            ui.separator();

            let load_btn = ui.add_enabled(!app.is_loading_groups, egui::Button::new("Load groups"));
            if load_btn.clicked() {
                app.start_load_log_groups();
            }

            if app.is_loading_groups {
                ui.spinner();
            }
        });

        // Third row: group + fetch.
        ui.horizontal(|ui| {
            ui.label("Group:");

            let current_group_name = app
                .logs_view
                .selected_group_index
                .and_then(|idx| app.logs_view.available_groups.get(idx))
                .cloned()
                .unwrap_or_else(|| app.logs_view.log_group.clone());

            egui::ComboBox::from_id_salt("log_group_combo")
                .selected_text(if current_group_name.is_empty() {
                    "<none>"
                } else {
                    current_group_name.as_str()
                })
                .show_ui(ui, |ui| {
                    for (idx, name) in app.logs_view.available_groups.iter().enumerate() {
                        let selected = Some(idx) == app.logs_view.selected_group_index;
                        if ui.selectable_label(selected, name).clicked() {
                            app.logs_view.selected_group_index = Some(idx);
                            app.logs_view.log_group = name.clone();
                        }
                    }
                });

            ui.separator();

            let fetch_btn = ui.add_enabled(!app.is_fetching, egui::Button::new("Fetch last 5m"));
            if fetch_btn.clicked() {
                app.start_fetch_logs(std::time::Duration::from_secs(5 * 60));
            }

            if app.is_fetching {
                ui.spinner();
            }
        });
    });
}
