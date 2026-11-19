use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Completion, Event, FieldSnapshot, log};
use pith_infer::Stage;

use super::App;
use super::RunError;
use crate::error::{InsertError, Inserted, ReaderError};
use crate::reader::Report;
use crate::reading::Reading;
use crate::x11::{DisplayError, Stroke};

pub enum Inbound {
    Report(Result<Report, ReaderError>),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Preparing(Stage),
    Key(Stroke),
    Failed(DisplayError),
    Quit,
}

#[derive(Clone)]
pub struct Inbox(Sender<Inbound>);

impl Inbox {
    pub fn channel() -> (Self, Receiver<Inbound>) {
        let (sender, receiver) = channel();
        (Self(sender), receiver)
    }

    pub fn post(&self, message: Inbound) {
        let _ = self.0.send(message);
    }
}

pub(super) enum Flow {
    Continue,
    Stop,
    Fail(RunError),
}

impl App {
    pub(super) fn handle(&mut self, message: Inbound) -> Flow {
        match message {
            Inbound::Report(Ok(Report::Reading(reading, served))) => self.reading(reading, served),
            Inbound::Report(Ok(Report::Inserted(outcome))) => self.inserted(outcome),
            Inbound::Report(Err(error)) => return Flow::Fail(error.into()),
            Inbound::Failed(error) => return Flow::Fail(error.into()),
            Inbound::Quit => return Flow::Stop,
            Inbound::Done(generation, result) => self.completion_done(generation, result),
            Inbound::Ready(status) => match status {
                Ok(name) => log(&format!("ready ({name})")),
                Err(error) => log(&format!("error: {error}")),
            },
            Inbound::Preparing(stage) => match stage {
                Stage::Downloading(progress) => {
                    log(&format!("downloading model, {}%", progress.percent()));
                }
                Stage::Loading { file } => log(&format!("loading {file}")),
            },
            Inbound::Key(stroke) => self.key_down(stroke),
        }
        Flow::Continue
    }

    fn reading(&mut self, reading: Reading, served: Option<u64>) {
        let snapshot: Option<FieldSnapshot> = match reading {
            Reading::Field(field) => Some(*field),
            Reading::Nothing => None,
        };
        self.effects
            .verify(snapshot.as_ref().map(|field| field.before.as_str()));
        self.dispatch(Event::Field(snapshot));
        if self.keyboard.serves(served) {
            self.release_keys();
        }
    }

    fn inserted(&mut self, outcome: Result<Inserted, InsertError>) {
        self.effects.inserted(outcome);
        if !self.effects.inserting() {
            self.keyboard.let_go();
            self.timers.clear_key_deadline();
        }
    }

    fn completion_done(&mut self, generation: u64, result: Result<Completion, String>) {
        self.effects.log_result(&result);
        self.dispatch(Event::Completed { generation, result });
    }
}
