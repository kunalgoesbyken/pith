use std::cell::RefCell;

use pith_core::{PauseSpan, Reach};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::HWND;

use crate::reach::tooltip;
use windows::Win32::UI::Shell::{
    NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};

pub const POWER_TOOLTIP: &str = "Pith \u{b7} paused while battery saver is on";
const MENU_TEXT_CHARS: usize = 100;

fn menu_text(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut shown: String = flat.chars().take(MENU_TEXT_CHARS).collect();
    if flat.chars().count() > MENU_TEXT_CHARS {
        shown.push('\u{2026}');
    }
    shown.replace('&', "&&")
}

pub enum TrayCommand {
    Pause(PauseSpan),
    Resume,
    BlockForeground,
    Settings,
    Quit,
    ResumeApp(String),
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrayView {
    pub status: String,
    pub active: bool,
    pub paused: bool,
    pub power_paused: bool,
    pub foreground: Option<String>,
    pub foreground_blocked: bool,
}

impl TrayView {
    fn block_label(&self) -> String {
        match (&self.foreground, self.foreground_blocked) {
            (Some(app), true) => format!("Turned off in {app}"),
            (Some(app), false) => format!("Turn off in {app}"),
            (None, _) => "Turn off in this app".to_string(),
        }
    }

    fn can_block(&self) -> bool {
        self.foreground.is_some() && !self.foreground_blocked
    }
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
    pauses: Vec<(MenuItem, PauseSpan)>,
    resume: MenuItem,
    block: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
    resting_tooltip: String,
    shown: RefCell<Option<TrayView>>,
}

fn tooltip_for<'a>(view: &TrayView, resting: &'a str) -> &'a str {
    if view.power_paused {
        POWER_TOOLTIP
    } else {
        resting
    }
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
    pub fn create(active: bool, reach: &Reach) -> Option<Self> {
        let menu = Menu::new();
        let status = MenuItem::new("Starting", false, None);
        let pauses: Vec<(MenuItem, PauseSpan)> = PauseSpan::CHOICES
            .into_iter()
            .map(|span| (MenuItem::new(span.label(), true, None), span))
            .collect();
        let resume = MenuItem::new("Resume", false, None);
        let block = MenuItem::new(TrayView::default().block_label(), false, None);
        let settings = MenuItem::new("Settings\u{2026}", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&status).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        for (item, _) in &pauses {
            menu.append(item).ok()?;
        }
        menu.append(&resume).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&block).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&settings).ok()?;
        menu.append(&quit).ok()?;
        let resting_tooltip = tooltip(reach);
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(&resting_tooltip)
            .with_icon(glyph(active))
            .build()
            .ok()?;
        Some(Self {
            menu,
            resumes: RefCell::new(Vec::new()),
            icon,
            status,
            pauses,
            resume,
            block,
            settings,
            quit,
            resting_tooltip,
            shown: RefCell::new(None),
        })
    }

    pub fn set_auto_paused(&self, apps: &[String]) {
        let mut resumes = self.resumes.borrow_mut();
        for (item, _) in resumes.drain(..) {
            let _ = self.menu.remove(&item);
        }
        for (position, app) in apps.iter().enumerate() {
            let label = format!("Paused in {} (ignored) \u{2014} Resume", menu_text(app));
            let item = MenuItem::new(label, true, None);
            if self.menu.insert(&item, 1 + position).is_ok() {
                resumes.push((item, app.clone()));
            }
        }
    }

    pub fn show(&self, view: TrayView) {
        let mut shown = self.shown.borrow_mut();
        let before = shown.as_ref();
        if before == Some(&view) {
            return;
        }
        if before.is_none_or(|old| old.status != view.status) {
            self.status.set_text(menu_text(&view.status));
        }
        if before.is_none_or(|old| old.active != view.active) {
            let _ = self.icon.set_icon(Some(glyph(view.active)));
        }
        if before.is_none_or(|old| old.power_paused != view.power_paused) {
            let _ = self
                .icon
                .set_tooltip(Some(tooltip_for(&view, &self.resting_tooltip)));
        }
        if before.is_none_or(|old| old.paused != view.paused) {
            self.resume.set_enabled(view.paused);
        }
        if before.is_none_or(|old| {
            old.foreground != view.foreground || old.foreground_blocked != view.foreground_blocked
        }) {
            self.block.set_text(menu_text(&view.block_label()));
            self.block.set_enabled(view.can_block());
        }
        *shown = Some(view);
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
            return Some(TrayCommand::ResumeApp(app.clone()));
        }
        if let Some((_, span)) = self.pauses.iter().find(|(item, _)| event.id == *item.id()) {
            Some(TrayCommand::Pause(*span))
        } else if event.id == *self.resume.id() {
            Some(TrayCommand::Resume)
        } else if event.id == *self.block.id() {
            Some(TrayCommand::BlockForeground)
        } else if event.id == *self.settings.id() {
            Some(TrayCommand::Settings)
        } else if event.id == *self.quit.id() {
            Some(TrayCommand::Quit)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_item_names_the_app_and_only_arms_for_one_not_yet_off() {
        let mut view = TrayView::default();
        assert_eq!(view.block_label(), "Turn off in this app");
        assert!(!view.can_block());
        view.foreground = Some("Slack".into());
        assert_eq!(view.block_label(), "Turn off in Slack");
        assert!(view.can_block());
        view.foreground_blocked = true;
        assert_eq!(view.block_label(), "Turned off in Slack");
        assert!(!view.can_block());
    }

    #[test]
    fn menu_text_escapes_ampersands_and_flattens_long_multiline_text() {
        assert_eq!(menu_text("Turn off in R&D"), "Turn off in R&&D");
        assert_eq!(menu_text("a\n  |\n1 | b\tc"), "a | 1 | b c");
        let long = "x".repeat(300);
        assert_eq!(menu_text(&long).chars().count(), MENU_TEXT_CHARS + 1);
        assert!(menu_text(&long).ends_with('\u{2026}'));
    }
}
