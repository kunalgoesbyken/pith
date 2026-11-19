use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Completion, Event, FieldSnapshot, Model, TrayCommand, log};
use pith_infer::Stage;

use super::App;
use super::RunError;
use super::keyboard::HOLD_LIMIT;
use crate::error::{InsertError, Inserted, ReaderError, Target};
use crate::notice::notify;
use crate::reader::Report;
use crate::reading::Reading;
use crate::tray::Tray;
use crate::x11::{DisplayError, Stroke};

pub enum Inbound {
    Report(Result<Report, ReaderError>),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Preparing(Stage),
    Command(TrayCommand),
    Tray(Tray),
    Key(Stroke),
    Failed(DisplayError),
    Unpasted,
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
            Inbound::Unpasted => self.reader.refresh(),
            Inbound::Quit => return Flow::Stop,
            Inbound::Done(generation, result) => self.completion_done(generation, result),
            Inbound::Ready(status) => self.model_ready(status),
            Inbound::Preparing(stage) => self.model_preparing(stage),
            Inbound::Command(command) => return self.command(command),
            Inbound::Tray(tray) => self.tray_ready(tray),
            Inbound::Key(stroke) => self.key_down(stroke),
        }
        Flow::Continue
    }

    fn reading(&mut self, reading: Reading, served: Option<u64>) {
        let snapshot: Option<FieldSnapshot> = match reading {
            Reading::Field(field) => Some(*field),
            Reading::Nothing => None,
        };
        self.current.set(snapshot.as_ref().map(|field| Target {
            field_id: field.field_id,
            app: field.app.clone(),
        }));
        if let Some(field) = &snapshot {
            self.shell.set_app(&field.app);
        }
        let config = self.engine.config();
        let usable = config.enabled
            && snapshot
                .as_ref()
                .is_some_and(|field| !config.is_blocked(&field.app));
        self.keyboard.keys.set_trigger(usable);
        self.dispatch(Event::Field(snapshot));
        if self.keyboard.serves(served) {
            self.release_keys();
        }
    }

    fn inserted(&mut self, outcome: Result<Inserted, InsertError>) {
        self.effects.inserted(outcome, self.current.get().as_ref());
        if !self.effects.inserting(HOLD_LIMIT) {
            self.keyboard.let_go();
            self.timers.clear_key_deadline();
        }
    }

    fn tray_ready(&mut self, tray: Tray) {
        log("tray icon ready");
        self.shell.set_tray(tray);
        self.refresh();
    }

    fn model_ready(&mut self, status: Result<String, String>) {
        match status {
            Ok(name) => {
                log(&format!("ready ({name})"));
                self.shell.set_model(Model::Ready);
            }
            Err(error) => {
                log(&format!("error: {error}"));
                if self.shell.is_new_failure(&error) {
                    notify(format!("Pith could not load its model: {error}"));
                }
                self.shell.set_model(Model::Failed(error));
            }
        }
        self.refresh();
    }

    fn model_preparing(&mut self, stage: Stage) {
        let model = match stage {
            Stage::Downloading(progress) => {
                log(&format!("downloading model, {}%", progress.percent()));
                Model::Downloading {
                    percent: progress.percent(),
                }
            }
            Stage::Loading { file } => {
                log(&format!("loading {file}"));
                Model::Loading
            }
        };
        self.shell.set_model(model);
        self.refresh();
    }

    fn completion_done(&mut self, generation: u64, result: Result<Completion, String>) {
        self.effects.log_result(&result);
        self.dispatch(Event::Completed { generation, result });
    }
}
