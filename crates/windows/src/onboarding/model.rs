use super::step::{Event, Step};
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Debug)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub step: Step,
    pub promise: String,
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
    pub fn advance(&mut self, event: Event) -> bool {
        let was_practice = self.step.practice_enabled();
        self.step = std::mem::replace(&mut self.step, Step::Preparing).next(event);
        self.step.practice_enabled() && !was_practice
    }

    pub fn page(&self) -> Option<Rgb> {
        (!self.mica).then_some(self.theme.palette.page)
    }

    pub fn card_surface(&self) -> Surface {
        Surface {
            theme: self.theme,
            scale: self.scale,
            backdrop: self.theme.palette.card,
        }
    }

    pub fn page_surface(&self) -> Surface {
        Surface {
            backdrop: self.theme.palette.page,
            ..self.card_surface()
        }
    }
}
