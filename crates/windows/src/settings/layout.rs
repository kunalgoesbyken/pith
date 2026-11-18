use windows::Win32::Foundation::RECT;

use pith_core::Counts;

use super::model::Model;
use super::{
    FREQUENCY, ID_DELETE_DATA, ID_ENABLED, ID_OPACITY_DOWN, ID_OPACITY_UP, ID_OPEN_DATA,
    ID_REMOVE_FIRST, ID_RESET_STATS, ID_START, LENGTH, MAX_APP_ROWS, Segment,
};
use crate::format::{duration, number};
use crate::ui::{BUTTON_HEIGHT, FOCUS_MARGIN, Role, Scale, expand, height, rect, toggle_width};

const COLUMN_WIDTH: i32 = 472;
const COLUMN_GAP: i32 = 24;
const MARGIN: i32 = 24;
pub const CLIENT_WIDTH: i32 = 2 * MARGIN + 2 * COLUMN_WIDTH + COLUMN_GAP;
const CARD_PAD: i32 = 16;
const ROW: i32 = 64;
const SLIM_ROW: i32 = 52;
const NOTE_ROW: i32 = 36;
const STAT_ROW: i32 = 36;
const HEADING_ROW: i32 = 32;
pub const FIGURE_COLUMN: i32 = 104;
const STEP_WIDTH: i32 = 40;
const SECTION_GAP: i32 = 20;
const LABEL_GAP: i32 = 8;
const HEADER_GAP: i32 = 4;
const SEGMENT_WIDTH: i32 = 64;
const BUTTON_WIDTH: i32 = 88;
const TOGGLE_LABEL: i32 = 28;
const CONTROL_GAP: i32 = 16;

pub const GENERAL_ROWS: [(&str, &str); 3] = [
    (
        "Suggest as I type",
        "Show the rest of your sentence in grey.",
    ),
    ("How often", "More shows suggestions sooner."),
    (
        "Start with Windows",
        "Open Pith in the tray when you sign in.",
    ),
];
pub const SUGGESTION_LABEL: &str = "Suggestions";
pub const LENGTH_ROW: (&str, &str) = ("Suggestion length", "Shorter or longer suggestions.");
pub const OPACITY_TITLE: &str = "Ghost text opacity";
pub const STATS_LABEL: &str = "Your stats";
pub const STATS_HEADINGS: (&str, &str) = ("Today", "All time");
pub const STATS_RESET: (&str, &str) = ("Reset stats", "The counts never leave this PC.");
pub const APPS_LABEL: &str = "Turned off in these apps";
pub const APPS_EMPTY: &str =
    "Nothing is turned off. Choose Turn off in this app from the tray menu.";
pub const DATA_LABEL: &str = "Your data";
pub const DATA_ROWS: [(&str, &str); 2] = [
    ("Open data folder", "The model and log live on this PC."),
    (
        "Delete model and data",
        "Settings are kept. The model downloads again.",
    ),
];
pub const FOOTER: &str = "Runs on this PC. Nothing leaves it.";
pub const TITLE: &str = "Pith";

#[derive(Clone, Debug)]
pub enum RowText {
    Pair(String, String),
    Title(String),
    Note(String),
    Heading(String, String),
    Figures(String, String, String),
}

#[derive(Clone, Debug)]
pub struct Row {
    pub bounds: RECT,
    pub text_bounds: RECT,
    pub text: RowText,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub label: Option<(RECT, &'static str)>,
    pub card: RECT,
    pub rows: Vec<Row>,
}

#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub id: i32,
    pub bounds: RECT,
    pub visible: bool,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub client: RECT,
    pub title: RECT,
    pub status: RECT,
    pub blocks: Vec<Block>,
    pub footer: RECT,
    pub controls: Vec<Placed>,
}

enum Control {
    None,
    Toggle(i32),
    Segments(&'static [Segment]),
    Stepper(i32, i32),
    Button(i32),
}

struct Spec {
    text: RowText,
    height: i32,
    control: Control,
}

struct Builder {
    scale: Scale,
    left: i32,
    inner: i32,
    y: i32,
    controls: Vec<Placed>,
}

impl Builder {
    fn px(&self, logical: i32) -> i32 {
        self.scale.px(logical)
    }

    fn take(&mut self, logical: i32) -> RECT {
        let bounds = rect(self.left, self.y, self.inner, self.px(logical));
        self.y += self.px(logical);
        bounds
    }

    fn place(&mut self, id: i32, visual: RECT) {
        let bounds = expand(visual, self.scale);
        self.controls.push(Placed {
            id,
            bounds,
            visible: true,
        });
    }

    fn segment_gap(&self) -> i32 {
        2 * self.px(FOCUS_MARGIN)
    }

    fn control_width(&self, control: &Control) -> i32 {
        match control {
            Control::None => 0,
            Control::Toggle(_) => toggle_width(self.scale, self.px(TOGGLE_LABEL)),
            Control::Segments(segments) => {
                let gaps = segments.len() as i32 - 1;
                segments.len() as i32 * self.px(SEGMENT_WIDTH) + gaps * self.segment_gap()
            }
            Control::Stepper(..) => 2 * self.px(STEP_WIDTH) + self.segment_gap(),
            Control::Button(_) => self.px(BUTTON_WIDTH),
        }
    }

    fn place_control(&mut self, control: &Control, row: RECT, right: i32) {
        let visual_height = self.px(BUTTON_HEIGHT);
        let top = row.top + (height(row) - visual_height) / 2;
        match control {
            Control::None => {}
            Control::Toggle(id) => {
                let span = self.control_width(control);
                self.place(*id, rect(right - span, top, span, visual_height));
            }
            Control::Segments(segments) => {
                let each = self.px(SEGMENT_WIDTH);
                let stride = each + self.segment_gap();
                let start = right - self.control_width(control);
                for (index, (id, ..)) in segments.iter().enumerate() {
                    let visual = rect(start + index as i32 * stride, top, each, visual_height);
                    self.place(*id, visual);
                }
            }
            Control::Stepper(down, up) => {
                let each = self.px(STEP_WIDTH);
                let start = right - self.control_width(control);
                self.place(*down, rect(start, top, each, visual_height));
                self.place(*up, rect(right - each, top, each, visual_height));
            }
            Control::Button(id) => {
                let span = self.px(BUTTON_WIDTH);
                self.place(*id, rect(right - span, top, span, visual_height));
            }
        }
    }

    fn card(&mut self, label: Option<&'static str>, specs: Vec<Spec>) -> Block {
        let label = label.map(|text| {
            let line = self.px(Role::BodyStrong.line());
            let bounds = rect(self.left, self.y, self.inner, line);
            self.y += line + self.px(LABEL_GAP);
            (bounds, text)
        });
        let top = self.y;
        let pad = self.px(CARD_PAD);
        let mut rows = Vec::new();
        for spec in specs {
            let bounds = self.take(spec.height);
            let right = bounds.right - pad;
            let span = self.control_width(&spec.control);
            self.place_control(&spec.control, bounds, right);
            let text_right = if span == 0 {
                right
            } else {
                right - span - self.px(CONTROL_GAP)
            };
            rows.push(Row {
                bounds,
                text_bounds: RECT {
                    left: bounds.left + pad,
                    top: bounds.top,
                    right: text_right,
                    bottom: bounds.bottom,
                },
                text: spec.text,
            });
        }
        let card = rect(self.left, top, self.inner, self.y - top);
        self.y += self.px(SECTION_GAP);
        Block { label, card, rows }
    }
}

fn pair(index: usize, rows: &[(&str, &str)]) -> RowText {
    let (title, caption) = rows[index];
    RowText::Pair(title.to_string(), caption.to_string())
}

fn stat_specs(model: &Model) -> Vec<Spec> {
    let both = |title: &str, value: &dyn Fn(&Counts) -> String| Spec {
        text: RowText::Figures(title.to_string(), value(&model.today), value(&model.total)),
        height: STAT_ROW,
        control: Control::None,
    };
    let saved = |counts: &Counts| duration(counts.seconds_saved(model.typing_wpm), number);
    vec![
        Spec {
            text: RowText::Heading(STATS_HEADINGS.0.to_string(), STATS_HEADINGS.1.to_string()),
            height: HEADING_ROW,
            control: Control::None,
        },
        both("Suggestions shown", &|counts| number(counts.shown)),
        both("Accepted in full", &|counts| number(counts.accepted)),
        both("Accepted word by word", &|counts| {
            number(counts.accepted_words)
        }),
        both("Words added", &|counts| number(counts.words)),
        both("Time saved", &saved),
        Spec {
            text: pair(0, &[STATS_RESET]),
            height: ROW,
            control: Control::Button(ID_RESET_STATS),
        },
    ]
}

fn app_specs(model: &Model) -> Vec<Spec> {
    let apps = model.visible_apps();
    if apps.is_empty() {
        return vec![Spec {
            text: RowText::Note(APPS_EMPTY.to_string()),
            height: SLIM_ROW,
            control: Control::None,
        }];
    }
    let mut specs: Vec<Spec> = apps
        .iter()
        .enumerate()
        .map(|(slot, name)| Spec {
            text: RowText::Title(name.clone()),
            height: SLIM_ROW,
            control: Control::Button(ID_REMOVE_FIRST + slot as i32),
        })
        .collect();
    if model.blocked.len() > MAX_APP_ROWS {
        let end = model.first_app + apps.len();
        specs.push(Spec {
            text: RowText::Note(format!(
                "{}\u{2013}{} of {}. Scroll to see more.",
                model.first_app + 1,
                end,
                model.blocked.len()
            )),
            height: NOTE_ROW,
            control: Control::None,
        });
    }
    specs
}

pub fn build(model: &Model) -> Layout {
    let scale = model.scale;
    let margin = scale.px(MARGIN);
    let mut builder = Builder {
        scale,
        left: margin,
        inner: scale.px(COLUMN_WIDTH),
        y: margin,
        controls: Vec::new(),
    };
    let title = builder.take(Role::Subtitle.line());
    builder.y += scale.px(HEADER_GAP);
    let status = builder.take(Role::Caption.line());
    builder.y += scale.px(SECTION_GAP);
    let columns_top = builder.y;
    let general = builder.card(
        None,
        vec![
            Spec {
                text: pair(0, &GENERAL_ROWS),
                height: ROW,
                control: Control::Toggle(ID_ENABLED),
            },
            Spec {
                text: pair(1, &GENERAL_ROWS),
                height: ROW,
                control: Control::Segments(&FREQUENCY),
            },
            Spec {
                text: pair(2, &GENERAL_ROWS),
                height: ROW,
                control: Control::Toggle(ID_START),
            },
        ],
    );
    let suggestions = builder.card(
        Some(SUGGESTION_LABEL),
        vec![
            Spec {
                text: pair(0, &[LENGTH_ROW]),
                height: ROW,
                control: Control::Segments(&LENGTH),
            },
            Spec {
                text: RowText::Pair(
                    OPACITY_TITLE.to_string(),
                    format!("{}%", number(model.opacity.into())),
                ),
                height: ROW,
                control: Control::Stepper(ID_OPACITY_DOWN, ID_OPACITY_UP),
            },
        ],
    );
    let apps_specs = app_specs(model);
    let used = model.visible_apps().len();
    let apps = builder.card(Some(APPS_LABEL), apps_specs);
    for slot in used..MAX_APP_ROWS {
        builder.controls.push(Placed {
            id: ID_REMOVE_FIRST + slot as i32,
            bounds: rect(0, 0, 0, 0),
            visible: false,
        });
    }
    let left_end = builder.y;
    builder.y = columns_top;
    builder.left += builder.inner + builder.px(COLUMN_GAP);
    let stats = builder.card(Some(STATS_LABEL), stat_specs(model));
    let data = builder.card(
        Some(DATA_LABEL),
        vec![
            Spec {
                text: pair(0, &DATA_ROWS),
                height: ROW,
                control: Control::Button(ID_OPEN_DATA),
            },
            Spec {
                text: pair(1, &DATA_ROWS),
                height: ROW,
                control: Control::Button(ID_DELETE_DATA),
            },
        ],
    );
    builder.y = builder.y.max(left_end);
    builder.left = margin;
    let footer = builder.take(Role::Caption.line());
    Layout {
        client: rect(0, 0, scale.px(CLIENT_WIDTH), builder.y + margin),
        title,
        status,
        blocks: vec![general, suggestions, apps, stats, data],
        footer,
        controls: builder.controls,
    }
}
