use pith_core::{
    Config, Counts, Length, MAX_GHOST_OPACITY, MIN_GHOST_OPACITY, ModelPreset, Setting, Suggestions,
};

use super::{Choice, MAX_APP_ROWS, OPACITY_STEP, StatsView};
use crate::reach;
use crate::rival::Rival;
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppRule {
    Off(String),
    On(String),
}

impl AppRule {
    pub fn name(&self) -> &str {
        match self {
            Self::Off(name) | Self::On(name) => name,
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Off(name) => format!("{name} \u{b7} off"),
            Self::On(name) => format!("{name} \u{b7} on"),
        }
    }

    pub fn removal(&self) -> Setting {
        match self {
            Self::Off(name) => Setting::Unblock(name.clone()),
            Self::On(name) => Setting::Disallow(name.clone()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub enabled: bool,
    pub typo_fix: bool,
    pub level: Option<Suggestions>,
    pub length: Option<Length>,
    pub preset: Option<ModelPreset>,
    pub opacity: u8,
    pub today: Counts,
    pub total: Counts,
    pub typing_wpm: u32,
    pub start_with_windows: bool,
    pub apps: Vec<AppRule>,
    pub status: String,
    pub footer: String,
}

#[derive(Debug)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub snapshot: Snapshot,
    pub recommended: ModelPreset,
    pub first_app: usize,
    pub page_offset: i32,
    pub page_room: i32,
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

pub fn read<R>(view: impl FnOnce(&Model) -> R) -> Option<R> {
    STATE.with(|slot| slot.read(view))
}

pub fn update<R>(change: impl FnOnce(&mut Model) -> R) -> Option<R> {
    STATE.with(|slot| slot.update(change))
}

fn model_label(config: &Config, recommended: ModelPreset) -> String {
    match (&config.remote_url, &config.model_path) {
        (Some(_), _) => "Remote model".to_string(),
        (None, Some(_)) => "Custom model".to_string(),
        (None, None) => format!("{} model", config.preset(recommended).label()),
    }
}

impl Snapshot {
    pub fn of(
        config: &Config,
        recommended: ModelPreset,
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
    ) -> Self {
        Self {
            enabled: config.enabled,
            typo_fix: config.typo_fix,
            level: Suggestions::of(config),
            length: Length::of(config),
            preset: config.uses_preset().then(|| config.preset(recommended)),
            opacity: config.ghost_opacity,
            today: stats.today,
            total: stats.total,
            typing_wpm: config.typing_wpm,
            start_with_windows,
            apps: config
                .blocked_apps
                .iter()
                .cloned()
                .map(AppRule::Off)
                .chain(config.allowed_apps.iter().cloned().map(AppRule::On))
                .collect(),
            status: format!("{status} \u{b7} {}", model_label(config, recommended)),
            footer: reach::footer(&config.reach()),
        }
    }

    pub fn selected(&self, choice: Choice) -> bool {
        match choice {
            Choice::Frequency(level) => self.level == Some(level),
            Choice::Length(length) => self.length == Some(length),
            Choice::Model(preset) => self.preset == Some(preset),
        }
    }

    pub fn select(&mut self, choice: Choice) {
        match choice {
            Choice::Frequency(level) => self.level = Some(level),
            Choice::Length(length) => self.length = Some(length),
            Choice::Model(preset) => self.preset = Some(preset),
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
}

impl Model {
    pub fn new(
        config: &Config,
        recommended: ModelPreset,
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
        (theme, mica, scale): (Theme, bool, Scale),
    ) -> Self {
        Self {
            theme,
            mica,
            scale,
            snapshot: Snapshot::of(config, recommended, start_with_windows, status, stats),
            recommended,
            first_app: 0,
            page_offset: 0,
            page_room: 0,
            rival: None,
        }
    }

    pub fn refresh(
        &mut self,
        config: &Config,
        start_with_windows: bool,
        status: &str,
        stats: &StatsView,
    ) -> bool {
        let next = Snapshot::of(config, self.recommended, start_with_windows, status, stats);
        if next == self.snapshot {
            return false;
        }
        self.snapshot = next;
        self.first_app = self.first_app.min(self.last_first_app());
        true
    }

    fn last_first_app(&self) -> usize {
        self.snapshot.apps.len().saturating_sub(MAX_APP_ROWS)
    }

    pub fn scroll(&mut self, rows: isize) -> bool {
        let moved = self
            .first_app
            .saturating_add_signed(rows)
            .min(self.last_first_app());
        let changed = moved != self.first_app;
        self.first_app = moved;
        changed
    }

    pub fn scroll_page(&mut self, by: i32) -> bool {
        let next = self.page_offset.saturating_add(by).clamp(0, self.page_room);
        let changed = next != self.page_offset;
        self.page_offset = next;
        changed
    }

    pub fn set_page_room(&mut self, room: i32) {
        self.page_room = room.max(0);
        self.scroll_page(0);
    }

    pub fn visible_apps(&self) -> &[AppRule] {
        let apps = &self.snapshot.apps;
        let end = (self.first_app + MAX_APP_ROWS).min(apps.len());
        &apps[self.first_app.min(end)..end]
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
            ModelPreset::Standard,
            false,
            "Ready",
            &StatsView::default(),
            (Theme::detect(), false, Scale(96)),
        )
    }

    fn names(model: &Model) -> Vec<&str> {
        model.visible_apps().iter().map(AppRule::name).collect()
    }

    #[test]
    fn the_visible_window_of_apps_clamps_to_the_list_and_follows_a_shrinking_list() {
        let mut model = with_apps(5);
        assert_eq!(model.visible_apps().len(), MAX_APP_ROWS);
        model.scroll(100);
        assert_eq!(names(&model), ["app2", "app3", "app4"]);
        model.scroll(-100);
        assert_eq!(names(&model), ["app0", "app1", "app2"]);
        assert_eq!(with_apps(0).visible_apps().len(), 0);
        let mut model = with_apps(5);
        model.scroll(2);
        let config = Config {
            blocked_apps: vec!["only".into()],
            ..Config::default()
        };
        assert!(model.refresh(&config, true, "Paused", &StatsView::default()));
        assert_eq!(model.first_app, 0);
        assert_eq!(names(&model), ["only"]);
        assert!(model.snapshot.start_with_windows);
        assert!(!model.refresh(&config, true, "Paused", &StatsView::default()));
    }

    #[test]
    fn opacity_steps_stay_inside_the_valid_range_and_report_only_changes() {
        let mut snapshot = with_apps(0).snapshot;
        snapshot.opacity = 95;
        assert_eq!(snapshot.step_opacity(true), Some(100));
        assert_eq!(snapshot.step_opacity(true), None);
        snapshot.opacity = 14;
        assert_eq!(snapshot.step_opacity(false), Some(10));
        assert_eq!(snapshot.step_opacity(false), None);
        snapshot.opacity = 50;
        assert_eq!(snapshot.step_opacity(true), Some(60));
        assert_eq!(snapshot.step_opacity(false), Some(50));
    }
}
