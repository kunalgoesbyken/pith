use pith_core::{
    Config, Counts, Length, MAX_GHOST_OPACITY, MIN_GHOST_OPACITY, ModelPreset, Suggestions,
};

use super::{Choice, MAX_APP_ROWS, OPACITY_STEP, StatsView};
use crate::reach;
use crate::rival::Rival;
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub enabled: bool,
    pub level: Option<Suggestions>,
    pub length: Option<Length>,
    pub opacity: u8,
    pub today: Counts,
    pub total: Counts,
    pub typing_wpm: u32,
    pub start_with_windows: bool,
    pub blocked: Vec<String>,
    pub first_app: usize,
    pub status: String,
    pub footer: String,
    pub rival: Option<Rival>,
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
        stats: &StatsView,
        (theme, mica, scale): (Theme, bool, Scale),
    ) -> Self {
        let mut model = Self {
            theme,
            mica,
            scale,
            enabled: false,
            level: None,
            length: None,
            opacity: config.ghost_opacity,
            today: Counts::default(),
            total: Counts::default(),
            typing_wpm: config.typing_wpm,
            start_with_windows: false,
            blocked: Vec::new(),
            first_app: 0,
            status: String::new(),
            footer: String::new(),
            rival: None,
        };
        model.refresh(config, start_with_windows, status, stats);
        model
    }

    pub fn refresh(
        &mut self,
        config: &Config,
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
    ) {
        self.enabled = config.enabled;
        self.level = Suggestions::of(config);
        self.length = Length::of(config);
        self.opacity = config.ghost_opacity;
        self.today = stats.today;
        self.total = stats.total;
        self.typing_wpm = config.typing_wpm;
        self.start_with_windows = start_with_windows;
        self.blocked = config.blocked_apps.clone();
        self.status = format!("{status} \u{b7} {}", model_label(config));
        self.footer = reach::footer(&config.reach());
        self.first_app = self.first_app.min(self.last_first_app());
    }

    fn last_first_app(&self) -> usize {
        self.blocked.len().saturating_sub(MAX_APP_ROWS)
    }

    pub fn selected(&self, choice: Choice) -> bool {
        match choice {
            Choice::Frequency(level) => self.level == Some(level),
            Choice::Length(length) => self.length == Some(length),
        }
    }

    pub fn select(&mut self, choice: Choice) {
        match choice {
            Choice::Frequency(level) => self.level = Some(level),
            Choice::Length(length) => self.length = Some(length),
        }
    }

    pub fn step_opacity(&mut self, up: bool) -> Option<u8> {
        let step = i16::from(OPACITY_STEP) * if up { 1 } else { -1 };
        let stepped = (i16::from(self.opacity) + step)
            .clamp(i16::from(MIN_GHOST_OPACITY), i16::from(MAX_GHOST_OPACITY));
        let stepped = stepped as u8;
        (stepped != self.opacity).then(|| {
            self.opacity = stepped;
            stepped
        })
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
        Model::new(
            &config,
            false,
            "Ready",
            &StatsView::default(),
            (Theme::detect(), false, Scale(96)),
        )
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
        model.refresh(&config, true, "Paused", &StatsView::default());
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
        model.refresh(&remote, false, "Ready", &StatsView::default());
        assert_eq!(model.status, "Ready \u{b7} Remote model");
    }

    #[test]
    fn opacity_steps_stay_inside_the_valid_range_and_report_only_changes() {
        let mut model = with_apps(0);
        model.opacity = 95;
        assert_eq!(model.step_opacity(true), Some(100));
        assert_eq!(model.step_opacity(true), None);
        model.opacity = 14;
        assert_eq!(model.step_opacity(false), Some(10));
        assert_eq!(model.step_opacity(false), None);
        model.opacity = 50;
        assert_eq!(model.step_opacity(true), Some(60));
        assert_eq!(model.step_opacity(false), Some(50));
    }

    #[test]
    fn choices_select_independently_per_group() {
        let mut model = with_apps(0);
        let frequency = Choice::Frequency(Suggestions::More);
        let length = Choice::Length(Length::Long);
        assert!(!model.selected(frequency) && !model.selected(length));
        model.select(length);
        assert!(model.selected(length) && !model.selected(frequency));
        model.select(frequency);
        assert!(model.selected(length) && model.selected(frequency));
    }

    #[test]
    fn refresh_carries_the_stats_and_the_config_values() {
        let mut model = with_apps(0);
        let stats = StatsView {
            today: Counts {
                shown: 4,
                ..Counts::default()
            },
            total: Counts {
                shown: 9,
                ..Counts::default()
            },
        };
        let config = Config {
            ghost_opacity: 70,
            max_words: 10,
            typing_wpm: 80,
            ..Config::default()
        };
        model.refresh(&config, false, "Ready", &stats);
        assert_eq!((model.today.shown, model.total.shown), (4, 9));
        assert_eq!(model.opacity, 70);
        assert_eq!(model.length, Some(Length::Long));
        assert_eq!(model.typing_wpm, 80);
    }
}
