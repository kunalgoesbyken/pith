mod render;
mod typeset;

use std::sync::Arc;

use pith_core::{
    Drawn, Emphasis, Form, Label, PILL_FONT_PX, PillStyle, Placed, Placement, Rgb, Scheme, Shading,
    Surface, TextStyle, fit_cuts, inline_font_px, is_rtl, log,
};

use crate::x11::{Canvas, Desktop, Display, DisplayError, Monitor};

use render::{Subject, render};
use typeset::{Font, Typesetter};

struct Shot<'a> {
    label: &'a Label,
    form: Form,
    font: Font<'a>,
    placed: Placed,
    monitor: Monitor,
    color: Option<Rgb>,
    translucent: bool,
}

pub struct Overlay {
    display: Arc<Display>,
    canvas: Canvas,
    desktop: Desktop,
    typesetter: Typesetter,
    background: Rgb,
    opacity: u8,
}

impl Overlay {
    pub fn create(
        display: Arc<Display>,
        desktop: Desktop,
        opacity: u8,
    ) -> Result<Self, DisplayError> {
        Ok(Self {
            canvas: Canvas::create(display.clone())?,
            display,
            desktop,
            typesetter: Typesetter::new(),
            background: Rgb::WHITE,
            opacity,
        })
    }

    pub fn hide(&mut self) {
        if let Err(error) = self.canvas.hide() {
            log(&format!("overlay hide failed: {error}"));
        }
    }

    pub fn show(&mut self, cuts: &[String], placement: Placement, style: &TextStyle) -> Drawn {
        let translucent = self.canvas.has_alpha() && self.desktop.compositing();
        let form = Form::of(placement, translucent);
        let Some(monitor) = self.desktop.nearest(form.target()) else {
            self.hide();
            return Drawn::Nothing;
        };
        let font = Self::font(form, style, monitor);
        let measure = |segment: &str| self.typesetter.measure(&font, segment);
        let surface = Surface {
            work: monitor.work,
            scale: monitor.scale,
            field: None,
            line_height: font.line_height(),
            measure: &measure,
            elevated: translucent,
            keycap: None,
        };
        let labels: Vec<Label> = cuts.iter().map(|text| Label::cut(text)).collect();
        let Some((index, placed)) = fit_cuts(&labels, form, &surface) else {
            self.hide();
            return Drawn::Nothing;
        };
        if !self.canvas.visible() {
            self.sample(form, &labels[index].text);
        }
        let shot = Shot {
            label: &labels[index],
            form,
            font,
            placed,
            monitor,
            color: style.color,
            translucent,
        };
        match self.present(&shot) {
            Ok(()) => Drawn::Shown(index),
            Err(error) => {
                log(&format!("overlay failed: {error}"));
                self.hide();
                Drawn::Nothing
            }
        }
    }

    fn font<'a>(form: Form, style: &'a TextStyle, monitor: Monitor) -> Font<'a> {
        match form {
            Form::Ghost(caret) => Font {
                family: style.family.as_deref(),
                px: inline_font_px(caret, style, monitor.scale),
            },
            Form::UnderCaret(_) | Form::UnderField(_) => Font {
                family: None,
                px: monitor.scale.size(PILL_FONT_PX).round() as i32,
            },
        }
    }

    fn sample(&mut self, form: Form, text: &str) {
        let rtl = is_rtl(text);
        self.background = self
            .display
            .sample_background(form.target(), form.anchor(rtl), rtl)
            .unwrap_or(Rgb::WHITE);
    }

    fn present(&mut self, shot: &Shot) -> Result<(), DisplayError> {
        let Shot {
            label,
            form,
            font,
            placed,
            monitor,
            color,
            translucent,
        } = shot;
        let measure = |segment: &str| self.typesetter.measure(font, segment);
        let text = &label.text;
        let emphasis = Emphasis::of(
            text,
            &label.strong,
            placed,
            is_rtl(text),
            font.line_height(),
            &measure,
        );
        let shading = match form {
            Form::Ghost(_) => Shading::inline(*color, self.background, self.opacity),
            Form::UnderCaret(_) | Form::UnderField(_) => Shading::pill(
                PillStyle::of(Scheme::over(self.background)),
                placed,
                monitor.scale,
            ),
        };
        let pixels = render(
            &self.typesetter,
            &Subject {
                text,
                font: *font,
                placed,
                emphasis: &emphasis,
                shading: &shading,
            },
            (!translucent).then_some(self.background),
        );
        self.canvas
            .present(placed.destination, placed.size, &pixels)
    }
}
