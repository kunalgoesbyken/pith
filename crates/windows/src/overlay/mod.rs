mod anchored;
mod compose;
mod face;
mod fade;
mod hint;
mod inline;
mod pill;
mod preview;
mod raster;
mod scene;
mod screen;
mod window;

use pith_core::{Drawn, Form, Label, Placed, Placement, Rgb, TextStyle, inline_anchor};

use compose::{Request, compose, render};
use inline::Backdrop;
use scene::Frame;
use screen::{Monitor, ScreenDc, focused_field, sample_background};
use window::Layered;

pub use preview::{Picture, Sample, preview};
pub use window::OVERLAY_CLASS;

use crate::appearance::Appearance;

pub struct Overlay {
    window: Layered,
    background: Rgb,
    appearance: Appearance,
    opacity: u8,
}

impl Overlay {
    pub fn create(opacity: u8, appearance: Appearance) -> windows::core::Result<Self> {
        Ok(Self {
            window: Layered::create()?,
            background: Rgb::WHITE,
            appearance,
            opacity,
        })
    }

    pub fn set_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
    }

    pub fn show(
        &mut self,
        cuts: &[String],
        placement: Placement,
        style: &TextStyle,
        key_hint: Option<&str>,
    ) -> Drawn {
        let labels: Vec<Label> = cuts.iter().map(|text| Label::cut(text)).collect();
        self.draw(&labels, placement, style, key_hint)
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
            None,
        )
    }

    fn draw(
        &mut self,
        labels: &[Label],
        placement: Placement,
        style: &TextStyle,
        key_hint: Option<&str>,
    ) -> Drawn {
        let request = Request {
            labels,
            placement,
            text: style,
            theme: self.appearance.theme,
            key_hint,
        };
        let screen = ScreenDc::get();
        let monitor = Monitor::nearest(Form::of(placement, true).target());
        compose(
            &request,
            monitor,
            &screen,
            focused_field(),
            |frame, placed, index| self.present(frame, &request, placed, index),
        )
        .unwrap_or_else(|| {
            self.window.hide();
            Drawn::Nothing
        })
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }

    fn capture(&mut self, frame: &Frame, placement: Placement) {
        if self.window.visible() {
            return;
        }
        if let Placement::Inline(caret) = placement {
            let anchor = inline_anchor(caret, frame.rtl);
            self.background = sample_background(frame.screen, caret, anchor, frame.rtl);
        }
    }

    fn present(&mut self, frame: &Frame, request: &Request, placed: Placed, index: usize) -> Drawn {
        self.capture(frame, request.placement);
        let backdrop = Backdrop {
            color: self.background,
            opacity: self.opacity,
        };
        match render(frame, request, placed, backdrop) {
            Some((raster, destination)) => {
                let animated = self.appearance.animated;
                self.window
                    .present(frame.screen, &raster, destination, animated);
                Drawn::Shown(index)
            }
            None => {
                self.window.hide();
                Drawn::Nothing
            }
        }
    }
}
