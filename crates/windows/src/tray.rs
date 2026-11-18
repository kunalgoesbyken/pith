use std::cell::RefCell;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};

pub enum TrayCommand {
    Toggle,
    OpenConfig,
    Quit,
    Resume(String),
}

fn toggle_label(enabled: bool) -> &'static str {
    if enabled { "Pause" } else { "Resume" }
}

fn fill(target: &mut [u16], text: &str) {
    let room = target.len() - 1;
    for (slot, unit) in target.iter_mut().take(room).zip(text.encode_utf16()) {
        *slot = unit;
    }
}

fn icon_id(hwnd: HWND) -> Option<u32> {
    (1..=256).find(|&id| {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: hwnd,
            uID: id,
            ..Default::default()
        };
        unsafe { Shell_NotifyIconGetRect(&identifier).is_ok() }
    })
}

pub struct Tray {
    menu: Menu,
    resumes: RefCell<Vec<(MenuItem, String)>>,
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
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("Pith")
            .with_icon(glyph(enabled))
            .build()
            .ok()?;
        Some(Self {
            menu,
            resumes: RefCell::new(Vec::new()),
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

    pub fn set_auto_paused(&self, apps: &[String]) {
        let mut resumes = self.resumes.borrow_mut();
        for (item, _) in resumes.drain(..) {
            let _ = self.menu.remove(&item);
        }
        for (position, app) in apps.iter().enumerate() {
            let name = app.replace('&', "&&");
            let label = format!("Paused in {name} (ignored) \u{2014} Resume");
            let item = MenuItem::new(label, true, None);
            if self.menu.insert(&item, 1 + position).is_ok() {
                resumes.push((item, app.clone()));
            }
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.toggle.set_text(toggle_label(enabled));
        let _ = self.icon.set_icon(Some(glyph(enabled)));
    }

    pub fn balloon(&self, title: &str, text: &str) {
        let hwnd = HWND(self.icon.window_handle());
        let Some(id) = icon_id(hwnd) else { return };
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: id,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        fill(&mut data.szInfoTitle, title);
        fill(&mut data.szInfo, text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    pub fn poll(&self) -> Option<TrayCommand> {
        let event = MenuEvent::receiver().try_recv().ok()?;
        if let Some((_, app)) = self
            .resumes
            .borrow()
            .iter()
            .find(|(item, _)| event.id == *item.id())
        {
            return Some(TrayCommand::Resume(app.clone()));
        }
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
