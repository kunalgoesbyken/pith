use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum TrayCommand {
    Toggle,
    OpenConfig,
    Quit,
}

fn toggle_label(enabled: bool) -> &'static str {
    if enabled { "Pause" } else { "Resume" }
}

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    toggle: MenuItem,
    config: MenuItem,
    quit: MenuItem,
}

fn glyph(active: bool) -> Icon {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let (r, g, b) = if active {
        (64, 158, 255)
    } else {
        (150, 150, 150)
    };
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let distance = (dx * dx + dy * dy).sqrt();
            let alpha = ((14.5 - distance).clamp(0.0, 1.0) * 255.0) as u8;
            let pith = (x as f32 - 15.5).abs() < 2.5 && (9..23).contains(&y);
            if pith {
                rgba.extend_from_slice(&[255, 255, 255, alpha])
            } else {
                rgba.extend_from_slice(&[r, g, b, alpha])
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon")
}

impl Tray {
    pub fn create(enabled: bool) -> Option<Self> {
        let menu = Menu::new();
        let status = MenuItem::new("Pith: starting…", false, None);
        let toggle = MenuItem::new(toggle_label(enabled), true, None);
        let config = MenuItem::new("Open settings", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&status).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&toggle).ok()?;
        menu.append(&config).ok()?;
        menu.append(&quit).ok()?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Pith")
            .with_icon(glyph(enabled))
            .build()
            .ok()?;
        Some(Self {
            icon,
            status,
            toggle,
            config,
            quit,
        })
    }

    pub fn set_status(&self, text: &str) {
        self.status.set_text(format!("Pith: {text}"));
        let _ = self.icon.set_tooltip(Some(format!("Pith: {text}")));
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.toggle.set_text(toggle_label(enabled));
        let _ = self.icon.set_icon(Some(glyph(enabled)));
    }

    pub fn poll(&self) -> Option<TrayCommand> {
        let event = MenuEvent::receiver().try_recv().ok()?;
        if event.id == *self.toggle.id() {
            Some(TrayCommand::Toggle)
        } else if event.id == *self.config.id() {
            Some(TrayCommand::OpenConfig)
        } else if event.id == *self.quit.id() {
            Some(TrayCommand::Quit)
        } else {
            None
        }
    }
}
