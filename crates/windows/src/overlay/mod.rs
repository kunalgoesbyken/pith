mod anchored;
mod face;
mod fade;
mod inline;
mod pill;
mod raster;
mod scene;
mod screen;
mod window;

use pith_core::{
    Drawn, Form, Label, PillStyle, Placed, Placement, Rgb, Surface, TextStyle, fit_cuts,
    inline_anchor, is_rtl,
};
use windows::Win32::Foundation::POINT;

use inline::Backdrop;
use raster::Raster;
use scene::{Frame, Scene};
use screen::{Monitor, ScreenDc, focused_field, sample_background};
use window::Layered;
pub use window::OVERLAY_CLASS;

use crate::ui::Theme;

pub struct Overlay {
    window: Layered,
    background: Rgb,
    style: PillStyle,
    opacity: u8,
}

impl Overlay {
    pub fn create(opacity: u8) -> windows::core::Result<Self> {
        Ok(Self {
            window: Layered::create()?,
            background: Rgb::WHITE,
            style: pill::style(&Theme::detect()),
            opacity,
        })
    }

    pub fn set_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    pub fn show(&mut self, cuts: &[String], placement: Placement, style: &TextStyle) -> Drawn {
        let labels: Vec<Label> = cuts.iter().map(|text| Label::cut(text)).collect();
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
        let form = Form::of(placement, true);
        let screen = ScreenDc::get();
        let monitor = Monitor::nearest(form.target());
        let face = match form {
            Form::Ghost(caret) => inline::face(&screen, monitor, caret, style),
            Form::UnderCaret(_) | Form::UnderField(_) => anchored::face(&screen, monitor),
        };
        let measure = |segment: &str| face.measure(segment);
        let surface = Surface {
            work: monitor.work,
            scale: monitor.scale,
            field: focused_field(),
            line_height: face.line_height,
            measure: &measure,
        };
        if let Some((index, placed)) = fit_cuts(labels, form, &surface) {
            let Label { text, strong } = &labels[index];
            let frame = Frame {
                text,
                strong,
                rtl: is_rtl(text),
                screen: &screen,
                monitor,
                face: &face,
            };
            return self.present(&frame, placement, style, placed, index);
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
                let anchor = inline_anchor(caret, frame.rtl);
                self.background = sample_background(frame.screen, caret, anchor, frame.rtl);
            }
            Placement::Anchored(_) | Placement::BelowCaret(_) => {
                self.style = pill::style(&Theme::detect());
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
        let destination = POINT {
            x: placed.destination.x,
            y: placed.destination.y,
        };
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
