use std::cell::RefCell;

use pith_core::{
    PauseSpan, QUIT_LABEL, RESUME_LABEL, Reach, TrayCommand, TrayView, auto_paused_label,
    menu_line, render_mark,
};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_INFO, NIIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};

use crate::appearance::Appearance;
use crate::brand;
use crate::reach::tooltip;
use crate::ui::Taskbar;
use crate::update::Update;

pub const POWER_TOOLTIP: &str = "Pith \u{b7} paused while battery saver is on";

fn menu_text(text: &str) -> String {
    menu_line(text).replace('&', "&&")
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrayFace {
    pub view: TrayView,
    pub taskbar: Taskbar,
    pub icon_px: u32,
    pub update: Update,
}

pub enum TrayEvent {
    Command(TrayCommand),
    Update,
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
    app: MenuItem,
    update: MenuItem,
    open: MenuItem,
    quit: MenuItem,
    resting_tooltip: String,
    shown: RefCell<Option<TrayFace>>,
}

fn tooltip_for<'a>(view: &TrayView, resting: &'a str) -> &'a str {
    if view.power_paused() {
        POWER_TOOLTIP
    } else {
        resting
    }
}

fn glyph(active: bool, taskbar: Taskbar, size: u32) -> Option<Icon> {
    let look = brand::tray(taskbar, active);
    Icon::from_rgba(render_mark(size, &look), size, size).ok()
}

impl Tray {
    pub fn create(active: bool, reach: &Reach, appearance: &Appearance) -> Option<Self> {
        let menu = Menu::new();
        let status = MenuItem::new("Starting", false, None);
        let pauses: Vec<(MenuItem, PauseSpan)> = PauseSpan::CHOICES
            .into_iter()
            .map(|span| (MenuItem::new(span.label(), true, None), span))
            .collect();
        let resume = MenuItem::new(RESUME_LABEL, false, None);
        let app = MenuItem::new(TrayView::default().app_label(), false, None);
        let update = MenuItem::new(Update::default().label(), true, None);
        let open = MenuItem::new("Open Pith", true, None);
        let quit = MenuItem::new(QUIT_LABEL, true, None);
        menu.append(&open).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&status).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        for (item, _) in &pauses {
            menu.append(item).ok()?;
        }
        menu.append(&resume).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&app).ok()?;
        menu.append(&update).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&quit).ok()?;
        let resting_tooltip = tooltip(reach);
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false)
            .with_tooltip(&resting_tooltip)
            .with_icon(glyph(active, appearance.taskbar, appearance.icon_px)?)
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
            update,
            open,
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
            let label = auto_paused_label(app).replace('&', "&&");
            let item = MenuItem::new(label, true, None);
            if self.menu.insert(&item, 1 + position).is_ok() {
                resumes.push((item, app.clone()));
            }
        }
    }

    pub fn show(&self, face: TrayFace) {
        let mut shown = self.shown.borrow_mut();
        let before = shown.as_ref();
        if before == Some(&face) {
            return;
        }
        let view = &face.view;
        if before.is_none_or(|old| old.view.status != view.status) {
            self.status.set_text(menu_text(&view.status));
        }
        let icon_changed = before.is_none_or(|old| {
            (old.view.active, old.taskbar, old.icon_px) != (view.active, face.taskbar, face.icon_px)
        });
        if icon_changed && let Some(icon) = glyph(view.active, face.taskbar, face.icon_px) {
            let _ = self.icon.set_icon(Some(icon));
        }
        if before.is_none_or(|old| old.view.power_paused() != view.power_paused()) {
            let _ = self
                .icon
                .set_tooltip(Some(tooltip_for(view, &self.resting_tooltip)));
        }
        if before.is_none_or(|old| old.view.resumable() != view.resumable()) {
            self.resume.set_enabled(view.resumable());
        }
        if before.is_none_or(|old| old.view.foreground != view.foreground) {
            self.app.set_text(menu_text(&view.app_label()));
            self.app.set_enabled(view.can_change_app());
        }
        if before.is_none_or(|old| old.update != face.update) {
            self.update.set_text(menu_text(&face.update.label()));
            self.update.set_enabled(face.update.can_act());
        }
        *shown = Some(face);
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

    fn clicked(&self) -> bool {
        let mut left_clicked = false;
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            left_clicked |= matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            );
        }
        left_clicked
    }

    pub fn poll(&self) -> Option<TrayEvent> {
        if self.clicked() {
            return Some(TrayEvent::Command(TrayCommand::Open));
        }
        let event = MenuEvent::receiver().try_recv().ok()?;
        if event.id == *self.update.id() {
            return Some(TrayEvent::Update);
        }
        self.command_of(&event.id).map(TrayEvent::Command)
    }

    fn command_of(&self, id: &MenuId) -> Option<TrayCommand> {
        if let Some((_, app)) = self
            .resumes
            .borrow()
            .iter()
            .find(|(item, _)| id == item.id())
        {
            return Some(TrayCommand::ResumeApp(app.clone()));
        }
        if let Some((_, span)) = self.pauses.iter().find(|(item, _)| id == item.id()) {
            Some(TrayCommand::Pause(*span))
        } else if id == self.resume.id() {
            Some(TrayCommand::Resume)
        } else if id == self.app.id() {
            Some(TrayCommand::ChangeApp)
        } else if id == self.open.id() {
            Some(TrayCommand::Open)
        } else if id == self.quit.id() {
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
    fn menu_text_escapes_ampersands() {
        assert_eq!(menu_text("Turn off in R&D"), "Turn off in R&&D");
    }
}
