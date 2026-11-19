use std::str::FromStr;

use pith_core::{CueKind, RawCue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    On,
    Noise,
    Cross,
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, String> {
        match name {
            "off" => Ok(Self::Off),
            "on" => Ok(Self::On),
            "noise" => Ok(Self::Noise),
            "cross" => Ok(Self::Cross),
            _ => Err(format!("unknown nearby mode {name}")),
        }
    }
}

fn label(text: &str) -> RawCue {
    RawCue::new(CueKind::FieldLabel, None, text)
}

fn value(name: &str, text: &str) -> RawCue {
    RawCue::new(CueKind::FormValue, Some(name), text)
}

fn text(text: &str) -> RawCue {
    RawCue::new(CueKind::Text, None, text)
}

fn message(text: &str) -> RawCue {
    RawCue::new(CueKind::Message, None, text)
}

fn own(sample: &str) -> Vec<RawCue> {
    match sample {
        "work-email" | "midline-close" => vec![
            label("Message Body"),
            value("To", "Sarah Chen"),
            value("Subject", "Re: Q3 vendor contract"),
            message(
                "Hi, please find attached the revised vendor contract with the updated terms for the third quarter. Let me know if everything looks right on your side.",
            ),
        ],
        "midline-numbers" => vec![
            label("Message Body"),
            value("To", "Dana Whitfield"),
            message("Could you send me the updated pricing before the end of the week? Thanks."),
        ],
        "casual-chat" => vec![
            label("Type a message"),
            text("Priya"),
            message("ok see you then!"),
        ],
        "slack" => vec![
            label("Message #design-team"),
            text("12 members"),
            message(
                "Copy for the landing page is approved. Still waiting on the final hero visuals.",
            ),
        ],
        "german" => vec![
            label("Nachrichtentext"),
            value("Betreff", "AW: Vertragsprüfung"),
            message(
                "Sehr geehrte Frau Becker, anbei sende ich Ihnen den überarbeiteten Vertragsentwurf mit den neuen Konditionen.",
            ),
        ],
        "spanish" => vec![
            label("Cuerpo del mensaje"),
            value("Para", "Marta Ruiz"),
            value("Asunto", "Reunión de seguimiento"),
            message("Hola, ¿podemos confirmar con tiempo la reunión de esta semana? Gracias."),
        ],
        "hindi" => vec![
            label("संदेश लिखें"),
            text("भैया"),
            message("ठीक है भैया, मैं घर पर ही रहूँगा।"),
        ],
        "japanese" => vec![
            label("本文"),
            value("宛先", "田中 太郎"),
            value("件名", "Re: ご依頼の件"),
            message("お世話になっております。先週の打ち合わせの件、ありがとうございました。"),
        ],
        "chinese" => vec![
            label("输入消息"),
            text("王经理"),
            message("好的，我们这边已经收到资料了，稍后回复您。"),
        ],
        "bullets" => vec![
            label("Type '/' for commands"),
            value("Attendees", "Daniel, Priya, Marco, Lena"),
            text("Team meeting notes"),
        ],
        "numbers-urls" => vec![
            label("Type '/' for commands"),
            value("Status", "Draft"),
            text("Product updates"),
        ],
        "long-document" => vec![label("Document content"), text("Last edit was seconds ago")],
        _ => Vec::new(),
    }
}

fn noise() -> Vec<RawCue> {
    [
        "Inbox 3",
        "Starred",
        "Snoozed",
        "Sent",
        "Drafts",
        "Spam",
        "Categories",
        "More",
    ]
    .into_iter()
    .map(text)
    .collect()
}

fn foreign(sample: &str) -> &'static str {
    match sample {
        "work-email" | "midline-close" | "midline-numbers" | "casual-chat" | "slack"
        | "bullets" | "numbers-urls" | "corrections" | "long-document" => "german",
        _ => "work-email",
    }
}

pub fn raw(sample: &str, mode: Mode) -> Vec<RawCue> {
    match mode {
        Mode::Off | Mode::On => own(sample),
        Mode::Noise => noise(),
        Mode::Cross => own(foreign(sample)),
    }
}
