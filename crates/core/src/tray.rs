use crate::{Access, Pause, PauseSpan, Setting};

const MENU_TEXT_CHARS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Pause(PauseSpan),
    Resume,
    ChangeApp,
    Open,
    Quit,
    ResumeApp(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForegroundApp {
    pub name: String,
    pub access: Access,
    pub switch: Option<Setting>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TrayView {
    pub status: String,
    pub active: bool,
    pub pause: Option<Pause>,
    pub foreground: Option<ForegroundApp>,
}

impl TrayView {
    pub fn app_label(&self) -> String {
        match &self.foreground {
            Some(ForegroundApp {
                name,
                access: Access::Off,
                switch: Some(_),
            }) => format!("Turn on in {name}"),
            Some(ForegroundApp {
                name,
                access: Access::Off,
                ..
            }) => format!("Turned off in {name}"),
            Some(ForegroundApp {
                name,
                access: Access::OffByDefault,
                ..
            }) => format!("Allow Pith in {name}"),
            Some(ForegroundApp { name, .. }) => format!("Turn off in {name}"),
            None => "Turn off in this app".to_string(),
        }
    }

    pub fn can_change_app(&self) -> bool {
        matches!(&self.foreground, Some(app) if app.switch.is_some())
    }

    pub fn resumable(&self) -> bool {
        matches!(self.pause, Some(Pause::Until(_) | Pause::UntilResumed))
    }

    pub fn power_paused(&self) -> bool {
        self.pause == Some(Pause::BatterySaver)
    }
}

pub fn menu_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut shown: String = flat.chars().take(MENU_TEXT_CHARS).collect();
    if flat.chars().count() > MENU_TEXT_CHARS {
        shown.push('\u{2026}');
    }
    shown
}

pub fn auto_paused_label(app: &str) -> String {
    format!("Paused in {} (ignored) \u{2014} Resume", menu_line(app))
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
            (Access::OffByDefault, "Allow Pith in Slack", true),
            (Access::Off, "Turned off in Slack", false),
        ] {
            view.foreground = Some(ForegroundApp {
                name: "Slack".into(),
                access,
                switch: (access != Access::Off).then(|| Setting::Block("Slack".into())),
            });
            assert_eq!(view.app_label(), label);
            assert_eq!(view.can_change_app(), armed);
        }
    }

    #[test]
    fn menu_line_flattens_long_multiline_text() {
        assert_eq!(menu_line("a\n  |\n1 | b\tc"), "a | 1 | b c");
        let long = "x".repeat(300);
        assert_eq!(menu_line(&long).chars().count(), MENU_TEXT_CHARS + 1);
        assert!(menu_line(&long).ends_with('\u{2026}'));
    }
}
