use pith_core::{Config, ModelPreset, Suggestions};

use super::MAX_APP_ROWS;
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub enabled: bool,
    pub level: Option<Suggestions>,
    pub start_with_windows: bool,
    pub blocked: Vec<String>,
    pub first_app: usize,
    pub status: String,
}

thread_local! {
    static STATE: Slot<Model> = const { Slot::new() };
}

pub fn install(model: Model) {
    STATE.with(|slot| slot.install(model));
}

pub fn clear() {
    STATE.with(Slot::clear);
}

pub fn current() -> Option<Model> {
    STATE.with(Slot::current)
}

pub fn update<R>(change: impl FnOnce(&mut Model) -> R) -> Option<R> {
    STATE.with(|slot| slot.update(change))
}

fn model_label(config: &Config) -> &'static str {
    match (&config.remote_url, config.model) {
        (Some(_), _) => "Remote model",
        (None, ModelPreset::Standard) => "Standard model",
        (None, ModelPreset::Lite) => "Lite model",
    }
}

impl Model {
    pub fn new(
        config: &Config,
        start_with_windows: bool,
        status: &str,
        (theme, mica, scale): (Theme, bool, Scale),
    ) -> Self {
        let mut model = Self {
            theme,
            mica,
            scale,
            enabled: false,
            level: None,
            start_with_windows: false,
            blocked: Vec::new(),
            first_app: 0,
            status: String::new(),
        };
        model.refresh(config, start_with_windows, status);
        model
    }

    pub fn refresh(&mut self, config: &Config, start_with_windows: bool, status: &str) {
        self.enabled = config.enabled;
        self.level = Suggestions::of(config);
        self.start_with_windows = start_with_windows;
        self.blocked = config.blocked_apps.clone();
        self.status = format!("{status} \u{b7} {}", model_label(config));
        self.first_app = self.first_app.min(self.last_first_app());
    }

    fn last_first_app(&self) -> usize {
        self.blocked.len().saturating_sub(MAX_APP_ROWS)
    }

    pub fn scroll(&mut self, rows: isize) {
        let moved = self.first_app.saturating_add_signed(rows);
        self.first_app = moved.min(self.last_first_app());
    }

    pub fn visible_apps(&self) -> &[String] {
        let end = (self.first_app + MAX_APP_ROWS).min(self.blocked.len());
        &self.blocked[self.first_app.min(end)..end]
    }

    pub fn surface(&self) -> Surface {
        Surface {
            theme: self.theme,
            scale: self.scale,
            backdrop: self.theme.palette.card,
        }
    }

    pub fn page(&self) -> Option<Rgb> {
        (!self.mica).then_some(self.theme.palette.page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_apps(apps: usize) -> Model {
        let config = Config {
            blocked_apps: (0..apps).map(|index| format!("app{index}")).collect(),
            ..Config::default()
        };
        Model::new(&config, false, "Ready", (Theme::detect(), false, Scale(96)))
    }

    #[test]
    fn the_visible_window_of_apps_clamps_to_the_list() {
        let mut model = with_apps(5);
        assert_eq!(model.visible_apps().len(), MAX_APP_ROWS);
        model.scroll(100);
        assert_eq!(model.visible_apps(), ["app2", "app3", "app4"]);
        model.scroll(-100);
        assert_eq!(model.visible_apps(), ["app0", "app1", "app2"]);
        assert_eq!(with_apps(0).visible_apps().len(), 0);
    }

    #[test]
    fn shrinking_the_list_pulls_the_window_back() {
        let mut model = with_apps(5);
        model.scroll(2);
        let config = Config {
            blocked_apps: vec!["only".into()],
            ..Config::default()
        };
        model.refresh(&config, true, "Paused");
        assert_eq!(model.first_app, 0);
        assert_eq!(model.visible_apps(), ["only"]);
        assert!(model.start_with_windows);
    }

    #[test]
    fn the_status_line_names_the_model() {
        let model = with_apps(0);
        assert_eq!(model.status, "Ready \u{b7} Standard model");
        let remote = Config {
            remote_url: Some("http://localhost".into()),
            ..Config::default()
        };
        let mut model = model;
        model.refresh(&remote, false, "Ready");
        assert_eq!(model.status, "Ready \u{b7} Remote model");
    }
}
