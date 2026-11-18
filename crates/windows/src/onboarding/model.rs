use super::step::View;
use crate::ui::{Rgb, Scale, Slot, Surface, Theme};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub theme: Theme,
    pub mica: bool,
    pub scale: Scale,
    pub view: View,
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

impl Model {
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
