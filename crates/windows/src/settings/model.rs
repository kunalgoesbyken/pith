use pith_core::{Config, KeyAction, ModelPreset};

use super::layout::{Class, Layout};
use super::page::Page;
use super::{Overview, UiEvent};
use crate::rival::Rival;
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Debug)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub viewport: (i32, i32),
    pub config: Config,
    pub overview: Overview,
    pub recommended: ModelPreset,
    pub cores: usize,
    pub page: Page,
    pub scroll: i32,
    pub room: i32,
    pub recording: Option<(KeyAction, usize)>,
    pub notice: Option<String>,
    pub preview: Option<u8>,
    pub rival: Option<Rival>,
    pub layout: Layout,
    pub live: Live,
}

#[derive(Debug, Default)]
pub struct Live {
    pub page: Page,
    pub controls: Vec<(i32, Class)>,
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

impl Model {
    pub fn new(
        (config, recommended): (&Config, ModelPreset),
        overview: &Overview,
        (theme, mica, scale): (Theme, bool, Scale),
        viewport: (i32, i32),
    ) -> Self {
        Self {
            theme,
            mica,
            scale,
            viewport,
            config: config.clone(),
            overview: overview.clone(),
            recommended,
            cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
            page: Page::Home,
            scroll: 0,
            room: 0,
            recording: None,
            notice: None,
            preview: None,
            rival: None,
            layout: Layout::default(),
            live: Live::default(),
        }
    }

    pub fn refresh(&mut self, config: &Config, overview: &Overview) -> bool {
        let unchanged = self.config == *config && self.overview == *overview;
        if unchanged {
            return false;
        }
        self.config = config.clone();
        self.overview = overview.clone();
        true
    }

    pub fn opacity(&self) -> u8 {
        self.preview.unwrap_or(self.config.ghost_opacity)
    }

    pub fn go(&mut self, page: Page) -> bool {
        let moved = self.page != page;
        self.page = page;
        self.scroll = 0;
        self.recording = None;
        self.notice = None;
        moved
    }

    pub fn scroll_by(&mut self, by: i32) -> bool {
        let next = self.scroll.saturating_add(by).clamp(0, self.room);
        let moved = next != self.scroll;
        self.scroll = next;
        moved
    }

    pub fn fit(&mut self, content_height: i32) -> bool {
        self.room = (content_height - self.viewport.1).max(0);
        self.scroll_by(0)
    }

    pub fn surface(&self, backdrop: Rgb) -> Surface {
        Surface {
            theme: self.theme,
            scale: self.scale,
            backdrop,
        }
    }

    pub fn page_fill(&self) -> Option<Rgb> {
        (!self.mica).then_some(self.theme.palette.page)
    }

    pub fn absorb(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Apply(setting) => setting.apply(&mut self.config),
            UiEvent::StartWithWindows(on) => self.overview.start_with_windows = *on,
            _ => {}
        }
    }
}
