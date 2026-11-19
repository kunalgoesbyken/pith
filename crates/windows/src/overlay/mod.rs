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

use pith_core::{Drawn, Placement, Rgb, TextStyle, drawn, is_rtl};

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

    pub fn show(&mut self, text: &str, placement: Placement, style: &TextStyle) -> Drawn {
        let (Placement::Inline(target)
        | Placement::Anchored(target)
        | Placement::BelowCaret(target)) = placement;
        let screen = ScreenDc::get();
        let frame = Frame {
            text,
            rtl: is_rtl(text),
            screen: &screen,
            monitor: Monitor::nearest(target),
        };
        self.capture(&frame, placement);
        match self.draw(&screen, &frame, placement, style) {
            Drawn::Prefix(len) => {
                let cut = Frame {
                    text: &text[..len],
                    ..frame
                };
                match self.draw(&screen, &cut, placement, style) {
                    Drawn::Whole => Drawn::Prefix(len),
                    Drawn::Prefix(_) | Drawn::Nothing => {
                        self.window.hide();
                        Drawn::Nothing
                    }
                }
            }
            outcome => outcome,
        }
    }

    fn draw(
        &mut self,
        screen: &ScreenDc,
        frame: &Frame,
        placement: Placement,
        style: &TextStyle,
    ) -> Drawn {
        let scene = match placement {
            Placement::Inline(caret) => {
                let backdrop = Backdrop {
                    color: self.background,
                    opacity: self.opacity,
                };
                inline::compose(frame, caret, style, backdrop)
            }
            Placement::Anchored(element) => anchored::under_field(frame, element, self.style),
            Placement::BelowCaret(caret) => {
                anchored::under_caret(frame, caret, focused_field(), self.style)
            }
        };
        let outcome = scene.as_ref().map_or(Drawn::Nothing, |scene| {
            drawn(frame.text, &scene.placed.lines)
        });
        match (outcome, scene) {
            (Drawn::Whole, Some(scene)) => self.present(screen, &scene),
            (Drawn::Prefix(len), _) => Drawn::Prefix(len),
            (Drawn::Whole | Drawn::Nothing, _) => {
                self.window.hide();
                Drawn::Nothing
            }
        }
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

    fn present(&mut self, screen: &ScreenDc, scene: &Scene) -> Drawn {
        match Raster::render(scene) {
            Some(raster) => {
                self.window
                    .present(screen, &raster, scene.placed.destination);
                Drawn::Whole
            }
            None => {
                self.window.hide();
                Drawn::Nothing
            }
        }
    }
}
