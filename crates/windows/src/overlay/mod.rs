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

use pith_core::{Placement, Rgb, TextStyle, is_rtl};

use inline::Backdrop;
use pill::Style;
use raster::Raster;
use scene::{Frame, Scene};
use screen::{Monitor, ScreenDc, sample_background};
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

    pub fn show(&mut self, text: &str, placement: Placement, style: &TextStyle) {
        let (Placement::Inline(target) | Placement::Anchored(target)) = placement;
        let screen = ScreenDc::get();
        let frame = Frame {
            text,
            rtl: is_rtl(text),
            screen: &screen,
            monitor: Monitor::nearest(target),
        };
        self.capture(&frame, placement);
        let scene = match placement {
            Placement::Inline(caret) => {
                let backdrop = Backdrop {
                    color: self.background,
                    opacity: self.opacity,
                };
                inline::compose(&frame, caret, style, backdrop)
            }
            Placement::Anchored(element) => anchored::compose(&frame, element, self.style),
        };
        self.present(&screen, scene);
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
            Placement::Anchored(_) => self.style = Style::of(&Theme::detect()),
        }
    }

    fn present(&mut self, screen: &ScreenDc, scene: Option<Scene>) {
        let drawn = scene
            .as_ref()
            .and_then(|scene| Some((Raster::render(scene)?, scene.placed.destination)));
        match drawn {
            Some((raster, destination)) => self.window.present(screen, &raster, destination),
            None => self.window.hide(),
        }
    }
}
