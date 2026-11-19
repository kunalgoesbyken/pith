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

use pith_core::{Drawn, Placement, Rgb, TextStyle, fits, is_rtl};

use inline::Backdrop;
use pill::Style;
use raster::Raster;
use scene::{Frame, Scene};
use screen::{Monitor, ScreenDc, focused_field, sample_background};
use window::Layered;

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
        let frame = |text| Frame {
            text,
            rtl: is_rtl(text),
            screen: &screen,
            monitor,
            face: &face,
            field,
        };
        if let Some(first) = cuts.first() {
            self.capture(&frame(first), placement);
        }
        for (index, text) in cuts.iter().enumerate() {
            let frame = frame(text);
            let scene = match placement {
                Placement::Inline(caret) => {
                    let backdrop = Backdrop {
                        color: self.background,
                        opacity: self.opacity,
                    };
                    inline::compose(&frame, caret, style, backdrop)
                }
                Placement::Anchored(element) => anchored::under_field(&frame, element, self.style),
                Placement::BelowCaret(caret) => anchored::under_caret(&frame, caret, self.style),
            };
            if let Some(scene) = scene.filter(|scene| fits(text, &scene.placed.lines)) {
                return self.present(&screen, &scene, index);
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

    fn present(&mut self, screen: &ScreenDc, scene: &Scene, index: usize) -> Drawn {
        match Raster::render(scene) {
            Some(raster) => {
                self.window
                    .present(screen, &raster, scene.placed.destination);
                Drawn::Shown(index)
            }
            None => {
                self.window.hide();
                Drawn::Nothing
            }
        }
    }
}
