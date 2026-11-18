use std::cell::RefCell;

use pith_core::PauseSpan;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const TOOLTIP: &str = "Pith \u{b7} local only, nothing leaves this PC";

pub enum TrayCommand {
    Pause(PauseSpan),
    Resume,
    BlockForeground,
    Settings,
    Quit,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrayView {
    pub status: String,
    pub active: bool,
    pub paused: bool,
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

pub struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    pauses: Vec<(MenuItem, PauseSpan)>,
    resume: MenuItem,
    block: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
    shown: RefCell<Option<TrayView>>,
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
    pub fn create(active: bool) -> Option<Self> {
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
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(TOOLTIP)
            .with_icon(glyph(active))
            .build()
            .ok()?;
        Some(Self {
            icon,
            status,
            pauses,
            resume,
            block,
            settings,
            quit,
            shown: RefCell::new(None),
        })
    }

    pub fn show(&self, view: TrayView) {
        let mut shown = self.shown.borrow_mut();
        let before = shown.as_ref();
        if before == Some(&view) {
            return;
        }
        if before.is_none_or(|old| old.status != view.status) {
            self.status.set_text(&view.status);
        }
        if before.is_none_or(|old| old.active != view.active) {
            let _ = self.icon.set_icon(Some(glyph(view.active)));
        }
        if before.is_none_or(|old| old.paused != view.paused) {
            self.resume.set_enabled(view.paused);
        }
        if before.is_none_or(|old| {
            old.foreground != view.foreground || old.foreground_blocked != view.foreground_blocked
        }) {
            self.block.set_text(view.block_label());
            self.block.set_enabled(view.can_block());
        }
        *shown = Some(view);
    }

    pub fn poll(&self) -> Option<TrayCommand> {
        let event = MenuEvent::receiver().try_recv().ok()?;
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
    fn tooltip_states_the_privacy_promise() {
        assert_eq!(TOOLTIP, "Pith \u{b7} local only, nothing leaves this PC");
    }
}
