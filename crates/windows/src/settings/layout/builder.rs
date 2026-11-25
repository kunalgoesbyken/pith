use windows::Win32::Foundation::RECT;

use super::grid::{
    BADGE_HEIGHT, BADGE_PAD, CARET_WIDTH, CONTROL_GAP, ITEM_GAP, MIN_TEXT_WIDTH, PANEL_INSET,
    ROW_MIN, ROW_MIN_CAPTION, ROW_PAD_X, ROW_PAD_Y, SAMPLE_HEIGHT, SECTION_BOTTOM, SECTION_TOP,
    STRIP_HEIGHT, TILE_GAP, TILE_HEIGHT, VALUE_WIDTH,
};
use super::item::{Face, Item};
use super::row::{KeyGroup, PlacedRow, PlacedSample, Row, Sample, Voice};
use super::{Block, Placed};
use crate::ui::{
    BUTTON_HEIGHT, FOCUS_MARGIN, KEYCAP_HEIGHT, Rgb, Role, SLIDER_WIDTH, STROKE, Scale,
    TOGGLE_HEIGHT, Text, button_width, chip_width, height, inset, key_labels, keycaps_width,
    measure, rect, segment_width, toggle_width, width,
};

pub const SEPARATOR: &str = "or";

pub struct Sheet {
    scale: Scale,
    backdrop: Rgb,
    left: i32,
    span: i32,
    y: i32,
    next_id: i32,
    blocks: Vec<Block>,
    controls: Vec<Placed>,
}

pub struct Built {
    pub blocks: Vec<Block>,
    pub controls: Vec<Placed>,
    pub bottom: i32,
}

pub struct Start {
    pub top: i32,
    pub first_id: i32,
}

struct RowMetrics {
    sizes: Vec<(i32, i32)>,
    gap: i32,
    all_chips: bool,
    right_width: i32,
    text_span: i32,
    title_line: i32,
    caption_height: i32,
    text_height: i32,
    height: i32,
}

struct RowText {
    title: Option<(RECT, String)>,
    caption: Option<(RECT, String, Voice)>,
    badge: Option<(RECT, String)>,
}

struct RowControls {
    keys: Vec<KeyGroup>,
    separators: Vec<RECT>,
    value: Option<(RECT, String)>,
}

fn wrapped_height(scale: Scale, text: &str, span: i32) -> i32 {
    let style = Text::new(Role::Caption, Rgb::BLACK).wrapping();
    measure(scale, text, style, span).1
}

fn text_width(scale: Scale, role: Role, text: &str) -> i32 {
    measure(scale, text, Text::new(role, Rgb::BLACK), 0).0
}

impl Sheet {
    pub fn new(scale: Scale, backdrop: Rgb, column: (i32, i32), start: Start) -> Self {
        Self {
            scale,
            backdrop,
            left: column.0,
            span: column.1,
            y: start.top,
            next_id: start.first_id,
            blocks: Vec::new(),
            controls: Vec::new(),
        }
    }

    pub fn finish(self) -> Built {
        Built {
            blocks: self.blocks,
            controls: self.controls,
            bottom: self.y,
        }
    }

    fn px(&self, logical: i32) -> i32 {
        self.scale.px(logical)
    }

    pub fn heading(&mut self, text: &str) {
        self.y += self.px(SECTION_TOP);
        let bounds = rect(
            self.left,
            self.y,
            self.span,
            self.px(Role::BodyStrong.line()),
        );
        self.y = bounds.bottom + self.px(SECTION_BOTTOM);
        self.blocks.push(Block::Heading(bounds, text.to_string()));
    }

    pub fn note(&mut self, text: &str, voice: Voice) {
        self.y += self.px(SECTION_TOP);
        let bounds = rect(
            self.left,
            self.y,
            self.span,
            wrapped_height(self.scale, text, self.span),
        );
        self.y = bounds.bottom;
        self.blocks
            .push(Block::Note(bounds, text.to_string(), voice));
    }

    fn place(&mut self, item: Item, bounds: RECT) {
        self.next_id += 1;
        self.controls.push(Placed {
            id: self.next_id,
            bounds,
            item,
            backdrop: self.backdrop,
        });
    }

    fn item_size(&self, item: &Item, segment: i32) -> (i32, i32) {
        let scale = self.scale;
        let margins = 2 * self.px(FOCUS_MARGIN);
        match &item.face {
            Face::Toggle => (
                toggle_width(scale) + margins,
                self.px(TOGGLE_HEIGHT) + margins,
            ),
            Face::Segment(_) => (segment, self.px(BUTTON_HEIGHT)),
            Face::Button(_) => (
                button_width(scale, &item.label) + margins,
                self.px(BUTTON_HEIGHT) + margins,
            ),
            Face::Chip(state) => (
                chip_width(scale, *state, &item.label) + margins,
                self.px(KEYCAP_HEIGHT) + margins,
            ),
            Face::Slider { .. } => (self.px(SLIDER_WIDTH) + margins, self.px(BUTTON_HEIGHT)),
            Face::Nav { .. } | Face::Tile(_) | Face::Strip(_) => (0, 0),
        }
    }

    fn separator_width(&self) -> i32 {
        text_width(self.scale, Role::Caption, SEPARATOR) + 2 * self.px(ITEM_GAP)
    }

    fn keys_width(&self, chords: &[String]) -> i32 {
        let caps: i32 = chords
            .iter()
            .map(|chord| keycaps_width(self.scale, key_labels(chord)))
            .sum();
        caps + self.separator_width() * chords.len().saturating_sub(1) as i32
    }

    pub fn card(&mut self, rows: Vec<Row>) {
        let top = self.y;
        let placed: Vec<PlacedRow> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| self.row(row, index > 0))
            .collect();
        let bounds = rect(self.left, top, self.span, self.y - top);
        self.blocks.push(Block::Card(bounds, placed));
    }

    fn row(&mut self, mut row: Row, divided: bool) -> PlacedRow {
        let metrics = self.measure_row(&row);
        let bounds = rect(self.left, self.y, self.span, metrics.height);
        self.y = bounds.bottom;
        let text = self.place_text(&row, bounds, &metrics);
        let sample = row
            .sample
            .take()
            .map(|sample| self.place_sample(sample, bounds));
        let controls = self.place_controls(row, bounds, &metrics);
        PlacedRow {
            divider: divided.then(|| self.divider(bounds)),
            title: text.title,
            caption: text.caption,
            badge: text.badge,
            keys: controls.keys,
            separators: controls.separators,
            value: controls.value,
            sample,
        }
    }

    fn measure_row(&self, row: &Row) -> RowMetrics {
        let segment = segment_width(self.scale, row.items.iter().map(|item| item.label.as_str()));
        let sizes: Vec<(i32, i32)> = row
            .items
            .iter()
            .map(|item| self.item_size(item, segment))
            .collect();
        let all_chips = row.items.len() > 1
            && row
                .items
                .iter()
                .all(|item| matches!(item.face, Face::Chip(_)));
        let gap = match (row.joined, all_chips) {
            (true, _) => 0,
            (false, true) => self.separator_width(),
            (false, false) => self.px(ITEM_GAP),
        };
        let items_width = sizes.iter().map(|size| size.0).sum::<i32>()
            + gap * sizes.len().saturating_sub(1) as i32;
        let value_width = row
            .value
            .as_ref()
            .map_or(0, |_| self.px(VALUE_WIDTH) + self.px(ITEM_GAP));
        let right_width = items_width + value_width + self.keys_width(&row.keys);
        let reserved = if right_width > 0 {
            right_width + self.px(CONTROL_GAP)
        } else {
            0
        };
        let pad = self.px(ROW_PAD_X);
        let text_span = (self.span - 2 * pad - reserved).max(self.px(MIN_TEXT_WIDTH));
        let caption_height = row
            .caption
            .as_ref()
            .map_or(0, |(text, _)| wrapped_height(self.scale, text, text_span));
        let title_line = if row.title.is_empty() {
            0
        } else {
            self.px(Role::Body.line())
        };
        let minimum = match (&row.sample, &row.caption) {
            (Some(_), _) => self.px(SAMPLE_HEIGHT),
            (None, Some(_)) => self.px(ROW_MIN_CAPTION),
            (None, None) => self.px(ROW_MIN),
        };
        let text_height = title_line + caption_height;
        RowMetrics {
            sizes,
            gap,
            all_chips,
            right_width,
            text_span,
            title_line,
            caption_height,
            text_height,
            height: (text_height + 2 * self.px(ROW_PAD_Y)).max(minimum),
        }
    }

    fn divider(&self, bounds: RECT) -> RECT {
        let pad = self.px(ROW_PAD_X);
        rect(
            bounds.left + pad,
            bounds.top,
            width(bounds) - pad,
            self.px(STROKE),
        )
    }

    fn place_text(&self, row: &Row, bounds: RECT, metrics: &RowMetrics) -> RowText {
        let text_top = bounds.top + (metrics.height - metrics.text_height) / 2;
        let text_left = bounds.left + self.px(ROW_PAD_X);
        let title_line = metrics.title_line;
        let title = (title_line > 0).then(|| {
            let area = rect(text_left, text_top, metrics.text_span, title_line);
            (area, row.title.clone())
        });
        let caption = row.caption.as_ref().map(|(text, voice)| {
            let area = rect(
                text_left,
                text_top + title_line,
                metrics.text_span,
                metrics.caption_height,
            );
            (area, text.clone(), *voice)
        });
        let badge = row.badge.as_ref().map(|badge| {
            let width = text_width(self.scale, Role::Caption, badge) + 2 * self.px(BADGE_PAD);
            let room = (metrics.text_span - width - self.px(ITEM_GAP)).max(0);
            let shown = text_width(self.scale, Role::Body, &row.title).min(room);
            let height = self.px(BADGE_HEIGHT);
            let left = text_left + shown + self.px(ITEM_GAP);
            let top = text_top + (title_line - height) / 2;
            (rect(left, top, width, height), badge.clone())
        });
        RowText {
            title,
            caption,
            badge,
        }
    }

    fn place_sample(&self, sample: Sample, bounds: RECT) -> PlacedSample {
        let pad = self.px(ROW_PAD_X);
        let area = RECT {
            left: bounds.left + pad,
            right: bounds.right - pad,
            ..bounds
        };
        let line = self.px(Role::Body.line());
        let typed = text_width(self.scale, Role::Body, &sample.typed);
        let caret = rect(
            area.left + typed,
            area.top + (height(area) - line) / 2,
            self.px(CARET_WIDTH),
            line,
        );
        PlacedSample {
            area,
            caret,
            sample,
        }
    }

    fn place_controls(&mut self, row: Row, bounds: RECT, metrics: &RowMetrics) -> RowControls {
        let row_height = height(bounds);
        let mut x = bounds.right - self.px(ROW_PAD_X) - metrics.right_width;
        let (keys, mut separators) = self.place_keys(&row.keys, &mut x, bounds);
        let line = self.px(Role::Caption.line());
        let gap = metrics.gap;
        let count = metrics.sizes.len();
        for (index, (item, size)) in row.items.into_iter().zip(&metrics.sizes).enumerate() {
            let top = bounds.top + (row_height - size.1) / 2;
            self.place(item, rect(x, top, size.0, size.1));
            x += size.0;
            if metrics.all_chips && index + 1 < count {
                let line_top = bounds.top + (row_height - line) / 2;
                separators.push(rect(x, line_top, gap, line));
            }
            x += gap;
        }
        let value = row.value.map(|text| {
            let top = bounds.top + (row_height - line) / 2;
            let area = rect(x + self.px(ITEM_GAP) - gap, top, self.px(VALUE_WIDTH), line);
            (area, text)
        });
        RowControls {
            keys,
            separators,
            value,
        }
    }

    fn place_keys(&self, chords: &[String], x: &mut i32, row: RECT) -> (Vec<KeyGroup>, Vec<RECT>) {
        let row_height = height(row);
        let cap_top = row.top + (row_height - self.px(KEYCAP_HEIGHT)) / 2;
        let line = self.px(Role::Caption.line());
        let mut groups = Vec::new();
        let mut separators = Vec::new();
        for (index, chord) in chords.iter().enumerate() {
            if index > 0 {
                let span = self.separator_width();
                separators.push(rect(*x, row.top + (row_height - line) / 2, span, line));
                *x += span;
            }
            groups.push(KeyGroup {
                origin: (*x, cap_top),
                chord: chord.clone(),
            });
            *x += keycaps_width(self.scale, key_labels(chord));
        }
        (groups, separators)
    }

    pub fn tiles(&mut self, tiles: Vec<Item>) {
        let gap = self.px(TILE_GAP);
        let count = tiles.len().max(1) as i32;
        let each = (self.span - gap * (count - 1)) / count;
        let pad = self.px(PANEL_INSET);
        let tile_height = self.px(TILE_HEIGHT);
        for (index, tile) in tiles.into_iter().enumerate() {
            let bounds = rect(
                self.left + index as i32 * (each + gap),
                self.y,
                each,
                tile_height,
            );
            self.blocks.push(Block::Panel(bounds));
            self.place(tile, inset(bounds, pad));
        }
        self.y += tile_height;
    }

    pub fn strip(&mut self, strip: Item) {
        let bounds = rect(self.left, self.y, self.span, self.px(STRIP_HEIGHT));
        self.blocks.push(Block::Panel(bounds));
        self.place(strip, inset(bounds, self.px(PANEL_INSET)));
        self.y = bounds.bottom;
    }
}
