use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};
use pith_core::{
    Look, PAUSED_GREY, PauseSpan, Scheme, TRAY_FILL, TrayCommand, TrayView, auto_paused_label, log,
    menu_line, render_mark,
};

use crate::autostart::Autostart;
use crate::run::{Inbound, Inbox};

const NAME: &str = "Pith";
const ICON_SIZES: [u32; 4] = [16, 24, 32, 48];

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Shown {
    pub view: TrayView,
    pub auto_paused: Vec<String>,
}

struct Menu {
    shown: Shown,
    autostart: Option<Autostart>,
    inbox: Inbox,
}

pub struct Tray(Handle<Menu>);

fn label(text: &str) -> String {
    menu_line(text).replace('_', "__")
}

fn icon(size: u32, active: bool) -> Icon {
    let colors = if active {
        [Scheme::DARK.accent; 3]
    } else {
        [PAUSED_GREY; 3]
    };
    let look = Look {
        plate: None,
        colors,
        fill: TRAY_FILL,
    };
    let rgba = render_mark(size, &look);
    let data = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, alpha]| [*alpha, *red, *green, *blue])
        .collect();
    let side = size as i32;
    Icon {
        width: side,
        height: side,
        data,
    }
}

fn command(text: String, enabled: bool, command: TrayCommand) -> MenuItem<Menu> {
    StandardItem {
        label: text,
        enabled,
        activate: Box::new(move |menu: &mut Menu| {
            menu.inbox.post(Inbound::Command(command.clone()));
        }),
        ..Default::default()
    }
    .into()
}

impl Menu {
    fn toggle_autostart(&mut self) {
        let Some(autostart) = &self.autostart else {
            return;
        };
        if let Err(error) = autostart.set(!autostart.enabled()) {
            log(&format!("start at login not changed: {error}"));
        }
    }

    fn start_at_login(&self) -> Option<MenuItem<Self>> {
        let autostart = self.autostart.as_ref()?;
        Some(
            CheckmarkItem {
                label: "Start at login".to_string(),
                checked: autostart.enabled(),
                activate: Box::new(Self::toggle_autostart),
                ..Default::default()
            }
            .into(),
        )
    }
}

impl ksni::Tray for Menu {
    fn id(&self) -> String {
        NAME.to_lowercase()
    }

    fn title(&self) -> String {
        NAME.to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        ICON_SIZES
            .map(|size| icon(size, self.shown.view.active))
            .into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: NAME.to_string(),
            description: self.shown.view.status.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.inbox.post(Inbound::Command(TrayCommand::Open));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let view = &self.shown.view;
        let mut items: Vec<MenuItem<Self>> = self
            .shown
            .auto_paused
            .iter()
            .map(|app| {
                command(
                    label(&auto_paused_label(app)),
                    true,
                    TrayCommand::ResumeApp(app.clone()),
                )
            })
            .collect();
        items.push(command(
            "Open settings".to_string(),
            true,
            TrayCommand::Open,
        ));
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: label(&view.status),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.extend(
            PauseSpan::CHOICES.map(|span| command(span.label(), true, TrayCommand::Pause(span))),
        );
        items.push(command(
            "Resume".to_string(),
            view.resumable(),
            TrayCommand::Resume,
        ));
        items.push(MenuItem::Separator);
        items.push(command(
            label(&view.app_label()),
            view.can_change_app(),
            TrayCommand::ChangeApp,
        ));
        items.extend(self.start_at_login());
        items.push(MenuItem::Separator);
        items.push(command("Quit".to_string(), true, TrayCommand::Quit));
        items
    }
}

impl Tray {
    pub fn spawn(inbox: Inbox, autostart: Option<Autostart>) -> Result<Self, ksni::Error> {
        Menu {
            shown: Shown::default(),
            autostart,
            inbox,
        }
        .spawn()
        .map(Self)
    }

    pub fn show(&self, shown: Shown) {
        self.0.update(|menu| menu.shown = shown);
    }
}
