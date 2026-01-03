use eframe::{NativeOptions, egui};

mod app;
mod aws;
mod tray;
mod worker;

use crate::app::App;
use crate::tray::{TrayConfig, TrayEventReceiver, TrayHandle};
use crate::worker::{WorkerHandle, spawn_worker};

struct AppShared {
    #[allow(dead_code)]
    tray_handle: TrayHandle,
    #[allow(dead_code)]
    tray_events: TrayEventReceiver,
    #[allow(dead_code)]
    worker_handle: WorkerHandle,
}

fn main() -> eframe::Result<()> {
    let worker_handle = spawn_worker();
    let tray_config = TrayConfig::default();
    let (tray_handle, tray_events) = TrayHandle::spawn(tray_config)
        .unwrap_or_else(|_err| (TrayHandle::dummy(), TrayEventReceiver::closed()));

    let shared = AppShared {
        tray_handle,
        tray_events,
        worker_handle: worker_handle.clone(),
    };

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 500.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Lumberjack Axe",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(cc, shared.worker_handle.clone())) as Box<dyn eframe::App>)
        }),
    )
}
