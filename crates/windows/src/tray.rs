use std::cell::RefCell;

use pith_core::{Access, Pause, PauseSpan, Reach};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

use crate::brand;
use crate::reach::tooltip;

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
    ChangeApp,
    Settings,
    Quit,
    ResumeApp(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct ForegroundApp {
    pub name: String,
    pub access: Access,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrayView {
    pub status: String,
    pub active: bool,
    pub pause: Option<Pause>,
    pub foreground: Option<ForegroundApp>,
}

impl TrayView {
    fn app_label(&self) -> String {
        match &self.foreground {
            Some(ForegroundApp {
                name,
                access: Access::Off,
            }) => format!("Turned off in {name}"),
            Some(ForegroundApp {
                name,
                access: Access::OffByDefault,
            }) => format!("Allow Pith in {name} (prose only)"),
            Some(ForegroundApp { name, .. }) => format!("Turn off in {name}"),
            None => "Turn off in this app".to_string(),
        }
    }

    fn can_change_app(&self) -> bool {
        matches!(&self.foreground, Some(app) if app.access != Access::Off)
    }

    fn resumable(&self) -> bool {
        matches!(self.pause, Some(Pause::Until(_) | Pause::UntilResumed))
    }

    fn power_paused(&self) -> bool {
        self.pause == Some(Pause::BatterySaver)
    }
}

fn fill(target: &mut [u16], text: &str) {
    let room = target.len() - 1;
    for (slot, unit) in target.iter_mut().take(room).zip(text.encode_utf16()) {
        *slot = unit;
    }
}

fn tray_icon_size() -> u32 {
    unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) as u32 }
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
    app: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
    resting_tooltip: String,
    shown: RefCell<Option<TrayView>>,
}

fn tooltip_for<'a>(view: &TrayView, resting: &'a str) -> &'a str {
    if view.power_paused() {
        POWER_TOOLTIP
    } else {
        resting
    }
}

fn glyph(active: bool) -> Icon {
    let size = tray_icon_size();
    let look = if active {
        &brand::ACTIVE
    } else {
        &brand::PAUSED
    };
    Icon::from_rgba(brand::render(size, look), size, size).expect("icon")
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
        let app = MenuItem::new(TrayView::default().app_label(), false, None);
        let settings = MenuItem::new("Settings\u{2026}", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&status).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        for (item, _) in &pauses {
            menu.append(item).ok()?;
        }
        menu.append(&resume).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&app).ok()?;
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
            app,
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
        if before.is_none_or(|old| old.power_paused() != view.power_paused()) {
            let _ = self
                .icon
                .set_tooltip(Some(tooltip_for(&view, &self.resting_tooltip)));
        }
        if before.is_none_or(|old| old.resumable() != view.resumable()) {
            self.resume.set_enabled(view.resumable());
        }
        if before.is_none_or(|old| old.foreground != view.foreground) {
            self.app.set_text(menu_text(&view.app_label()));
            self.app.set_enabled(view.can_change_app());
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
        } else if event.id == *self.app.id() {
            Some(TrayCommand::ChangeApp)
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
    fn app_item_names_the_app_and_arms_for_any_app_the_user_can_change() {
        let mut view = TrayView::default();
        assert_eq!(view.app_label(), "Turn off in this app");
        assert!(!view.can_change_app());
        for (access, label, armed) in [
            (Access::On, "Turn off in Slack", true),
            (Access::OptedIn, "Turn off in Slack", true),
            (
                Access::OffByDefault,
                "Allow Pith in Slack (prose only)",
                true,
            ),
            (Access::Off, "Turned off in Slack", false),
        ] {
            view.foreground = Some(ForegroundApp {
                name: "Slack".into(),
                access,
            });
            assert_eq!(view.app_label(), label);
            assert_eq!(view.can_change_app(), armed);
        }
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
