use crossbeam_channel::{Receiver, unbounded};
use std::fmt;
use tray_icon::{
    ClickType, Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuItem},
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum TrayEvent {
    ToggleWindow,
    ShowWindow,
    HideWindow,
    QuitRequested,
}

impl fmt::Display for TrayEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrayEvent::ToggleWindow => write!(f, "ToggleWindow"),
            TrayEvent::ShowWindow => write!(f, "ShowWindow"),
            TrayEvent::HideWindow => write!(f, "HideWindow"),
            TrayEvent::QuitRequested => write!(f, "QuitRequested"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrayConfig {
    pub tooltip: String,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            tooltip: "Lumberjack Axe".to_string(),
        }
    }
}

#[allow(dead_code)]
pub struct TrayHandle {
    inner: Option<TrayIcon>,
}

impl TrayHandle {
    pub fn spawn(config: TrayConfig) -> Result<(Self, TrayEventReceiver), TrayError> {
        // Channel from the tray callback to the rest of the app.
        let (sender, receiver) = unbounded::<TrayEvent>();

        // Minimal context menu for now: just a Quit item for future use.
        let menu = Menu::new();
        let _ = MenuItem::new("Quit", true, None);

        let mut builder = TrayIconBuilder::new();
        builder = builder.with_tooltip(config.tooltip);
        builder = builder.with_menu(Box::new(menu));

        if let Some(icon) = load_axe_icon() {
            builder = builder.with_icon(icon);
        }

        tray_icon::TrayIconEvent::set_event_handler(Some(Box::new(move |event: TrayIconEvent| {
            if event.click_type == ClickType::Left {
                let _ = sender.send(TrayEvent::ToggleWindow);
            }
        })));

        let icon = builder
            .build()
            .map_err(|e| TrayError::InitFailed(e.to_string()))?;

        let handle = TrayHandle { inner: Some(icon) };
        let receiver = TrayEventReceiver::new(receiver);

        Ok((handle, receiver))
    }

    pub fn dummy() -> Self {
        TrayHandle { inner: None }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum TrayError {
    Unsupported,
    InitFailed(String),
}

impl fmt::Display for TrayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrayError::Unsupported => write!(f, "tray not supported on this platform"),
            TrayError::InitFailed(reason) => {
                write!(f, "failed to initialize tray: {reason}")
            }
        }
    }
}

impl std::error::Error for TrayError {}

#[allow(dead_code)]
#[derive(Debug)]
pub struct TrayEventReceiver {
    inner: Option<Receiver<TrayEvent>>,
}

impl TrayEventReceiver {
    pub fn new(inner: Receiver<TrayEvent>) -> Self {
        Self { inner: Some(inner) }
    }

    pub fn closed() -> Self {
        Self { inner: None }
    }
}

fn load_axe_icon() -> Option<Icon> {
    let path = "assets/axe.png";
    match image::open(path) {
        Ok(img) => {
            // Resize to a small square icon for the tray.
            let img = img
                .resize_exact(32, 32, image::imageops::Lanczos3)
                .into_rgba8();
            let (width, height) = img.dimensions();
            let rgba = img.into_raw();

            match Icon::from_rgba(rgba, width, height) {
                Ok(icon) => {
                    println!("[axe] Loaded tray icon from {path} ({width}x{height})");
                    Some(icon)
                }
                Err(e) => {
                    eprintln!("[axe] Failed to create Icon from {path}: {e}");
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("[axe] Failed to open tray icon at {path}: {e}");
            None
        }
    }
}
