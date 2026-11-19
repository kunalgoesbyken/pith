use pith_core::Setting;

use super::super::action::Action;
use super::super::layout::{Item, Row, Sheet, Voice};
use super::super::model::Model;
use crate::ui::Tone;

const OFF_BY_DEFAULT: &str = "Off by default";

fn rule_row(name: &str, verb: &str, caption: &str, setting: Setting) -> Row {
    let button = Item::button(
        &format!("Turn {verb}"),
        Tone::Standard,
        Action::Apply(setting),
    )
    .named(&format!("Turn Pith {verb} in {name}"));
    Row::titled(name).caption(caption).control(button)
}

fn blocked_row(name: &str) -> Row {
    let setting = Setting::Unblock(name.to_string());
    rule_row(name, "on", "Pith is off here.", setting)
}

fn allowed_row(name: &str) -> Row {
    let setting = Setting::Disallow(name.to_string());
    rule_row(name, "off", "You turned Pith on in this editor.", setting).badge(OFF_BY_DEFAULT)
}

fn rule_rows(model: &Model) -> Vec<Row> {
    let config = &model.config;
    let rows: Vec<Row> = config
        .blocked_apps
        .iter()
        .map(|name| blocked_row(name))
        .chain(config.allowed_apps.iter().map(|name| allowed_row(name)))
        .collect();
    if !rows.is_empty() {
        return rows;
    }
    vec![
        Row::titled("No app has its own setting")
            .caption("Turn Pith off or on in an app from here, from Home or from the tray."),
    ]
}

pub fn build(model: &Model, sheet: &mut Sheet) {
    sheet.heading("Add an app");
    sheet.card(vec![
        Row::titled("Choose an app")
            .caption("Pick from the apps open right now.")
            .control(Item::button(
                "Choose app\u{2026}",
                Tone::Standard,
                Action::ChooseApp,
            )),
    ]);
    sheet.heading("Apps with their own setting");
    sheet.card(rule_rows(model));
    sheet.note(
        "Code editors start turned off so Pith does not compete with their own suggestions. Pick one above to turn it on.",
        Voice::Quiet,
    );
}
