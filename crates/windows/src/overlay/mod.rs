mod anchored;
mod emphasis;
mod face;
mod fade;
mod inline;
mod pill;
mod raster;
mod scene;
mod screen;
mod shading;
mod window;

use pith_core::{Drawn, Label, Placement, Rgb, TextStyle, fits, is_rtl, next_word};

use inline::Backdrop;
use pill::Style;
use raster::Raster;
use scene::{Frame, Placed, Scene};
use screen::{Monitor, ScreenDc, focused_field, sample_background};
use window::Layered;
pub use window::OVERLAY_CLASS;

use crate::ui::Theme;

pub struct Overlay {
    window: Layered,
    background: Rgb,
    style: Style,
    opacity: u8,
}

impl Overlay {
    pub fn create(opacity: u8) -> windows::core::Result<Self> {
        Ok(Self {
            window: Layered::create()?,
            background: Rgb::WHITE,
            style: Style::of(&Theme::detect()),
            opacity,
        })
    }

    pub fn set_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    pub fn show(&mut self, cuts: &[String], placement: Placement, style: &TextStyle) -> Drawn {
        let labels: Vec<Label> = cuts
            .iter()
            .map(|text| Label {
                strong: 0..next_word(text).len(),
                text: text.clone(),
            })
            .collect();
        self.draw(&labels, placement, style)
    }

    pub fn offer(&mut self, label: &Label, placement: Placement) -> Drawn {
        let placement = match placement {
            Placement::Inline(caret) => Placement::BelowCaret(caret),
            other => other,
        };
        self.draw(
            std::slice::from_ref(label),
            placement,
            &TextStyle::default(),
        )
    }

    fn draw(&mut self, labels: &[Label], placement: Placement, style: &TextStyle) -> Drawn {
        let (Placement::Inline(target)
        | Placement::Anchored(target)
        | Placement::BelowCaret(target)) = placement;
        let screen = ScreenDc::get();
        let monitor = Monitor::nearest(target);
        let face = match placement {
            Placement::Inline(caret) => inline::face(&screen, monitor, caret, style),
            Placement::Anchored(_) | Placement::BelowCaret(_) => anchored::face(&screen, monitor),
        };
        let field = focused_field();
        let frame = |text, strong| Frame {
            text,
            strong,
            rtl: is_rtl(text),
            screen: &screen,
            monitor,
            face: &face,
            field,
        };
        for (index, Label { text, strong }) in labels.iter().enumerate() {
            let frame = frame(text, strong);
            let placed = match placement {
                Placement::Inline(caret) => inline::place(&frame, caret),
                Placement::Anchored(element) => anchored::under_field(&frame, element),
                Placement::BelowCaret(caret) => anchored::under_caret(&frame, caret),
            };
            if let Some(placed) = placed.filter(|placed| fits(text, &placed.lines)) {
                return self.present(&frame, placement, style, placed, index);
            }
        }
        self.window.hide();
        Drawn::Nothing
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }

    fn capture(&mut self, frame: &Frame, placement: Placement) {
        if self.window.visible() {
            return;
        }
        match placement {
            Placement::Inline(caret) => {
                let anchor = inline::anchor(caret, frame.rtl);
                self.background = sample_background(frame.screen, caret, anchor, frame.rtl);
            }
            Placement::Anchored(_) | Placement::BelowCaret(_) => {
                self.style = Style::of(&Theme::detect());
            }
        }
    }

    fn present(
        &mut self,
        frame: &Frame,
        placement: Placement,
        style: &TextStyle,
        placed: Placed,
        index: usize,
    ) -> Drawn {
        self.capture(frame, placement);
        let shading = match placement {
            Placement::Inline(_) => {
                let backdrop = Backdrop {
                    color: self.background,
                    opacity: self.opacity,
                };
                inline::shading(style, backdrop)
            }
            Placement::Anchored(_) | Placement::BelowCaret(_) => {
                anchored::shading(frame, &placed, self.style)
            }
        };
        let destination = placed.destination;
        let scene = Scene::new(frame, placed, shading);
        match Raster::render(&scene) {
            Some(raster) => {
                self.window.present(frame.screen, &raster, destination);
                Drawn::Shown(index)
            }
            None => {
                self.window.hide();
                Drawn::Nothing
            }
        }
    }
}
