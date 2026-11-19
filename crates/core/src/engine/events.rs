use std::sync::Arc;

use crate::replacing::SelectOutcome;
use crate::{
    Completion, CompletionRequest, Config, Correction, Drawn, FieldSnapshot, Placement, Sample,
    Spelling, StyleMemory, TextStyle,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Field(Option<FieldSnapshot>),
    DebounceElapsed(u64),
    Completed {
        generation: u64,
        result: Result<Completion, String>,
    },
    AcceptAll,
    AcceptWord,
    Dismiss,
    Drawn(Drawn),
    Spelled {
        generation: u64,
        spelling: Spelling,
    },
    Selected {
        id: u64,
        outcome: SelectOutcome,
    },
    ReplaceTimedOut(u64),
    Trigger,
    Resume(String),
    Style(Arc<StyleMemory>),
    SetConfig(Config),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    ScheduleDebounce {
        generation: u64,
        delay_ms: u64,
    },
    StartCompletion {
        generation: u64,
        request: CompletionRequest,
    },
    CancelCompletion,
    CheckSpelling {
        generation: u64,
        language: String,
        word: String,
    },
    Offer {
        correction: Correction,
        placement: Placement,
    },
    Replace {
        id: u64,
        field_id: u64,
        correction: Correction,
    },
    ArmReplaceTimeout {
        id: u64,
        delay_ms: u64,
    },
    Overwrite(String),
    Collapse {
        field_id: u64,
        text: String,
    },
    Show {
        cuts: Vec<String>,
        placement: Placement,
        style: TextStyle,
    },
    Hide,
    Insert(String),
    ArmKeys,
    DisarmKeys,
    AutoPaused(Vec<String>),
    Learn(Sample),
}
