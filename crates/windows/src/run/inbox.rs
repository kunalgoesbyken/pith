use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Completion, Event, FieldSnapshot, Model, Spelling, log};
use pith_infer::{Stage, UpdateCheck, check_for_update};
use windows::Win32::Foundation::HWND;

use super::App;
use crate::control::Waker;
use crate::onboarding::Event as OnboardingEvent;
use crate::tray::TrayCommand;
use crate::update::Update;

pub enum Inbound {
    Snapshot(Option<FieldSnapshot>, Option<u64>),
    Done(u64, Result<Completion, String>),
    Spelled(u64, Spelling),
    Selected(u64, bool),
    Ready(Result<String, String>),
    Preparing(Stage),
    Command(TrayCommand),
    Update(UpdateCheck),
}

#[derive(Clone)]
pub struct Inbox {
    sender: Sender<Inbound>,
    waker: Waker,
}

impl Inbox {
    pub fn channel() -> (Self, Receiver<Inbound>) {
        let (sender, receiver) = channel();
        let inbox = Self {
            sender,
            waker: Waker::default(),
        };
        (inbox, receiver)
    }

    pub fn post(&self, message: Inbound) {
        let _ = self.sender.send(message);
        self.waker.wake();
    }

    pub(super) fn attach(&self, control: HWND) {
        self.waker.attach(control);
    }
}

impl App {
    pub(super) fn drain(&mut self) -> bool {
        let mut quit = false;
        while let Ok(message) = self.inbound.try_recv() {
            match message {
                Inbound::Snapshot(snapshot, served) => self.snapshot_read(snapshot, served),
                Inbound::Done(generation, result) => self.completion_done(generation, result),
                Inbound::Spelled(generation, spelling) => self.dispatch(Event::Spelled {
                    generation,
                    spelling,
                }),
                Inbound::Selected(generation, selected) => self.dispatch(Event::Selected {
                    generation,
                    selected,
                }),
                Inbound::Ready(status) => self.model_ready(status),
                Inbound::Preparing(stage) => self.model_preparing(stage),
                Inbound::Command(command) => quit |= self.command(command),
                Inbound::Update(result) => self.update_done(result),
            }
        }
        quit
    }

    pub(super) fn start_update_check(&mut self) {
        self.presenter.set_update(Update::Checking);
        self.refresh();
        let inbox = self.inbox.clone();
        std::thread::spawn(move || inbox.post(Inbound::Update(check_for_update())));
    }

    fn update_done(&mut self, result: UpdateCheck) {
        self.presenter.set_update(Update::Done(result));
        self.refresh();
    }

    fn snapshot_read(&mut self, snapshot: Option<FieldSnapshot>, served: Option<u64>) {
        self.dispatch(Event::Field(snapshot));
        if self.keyboard.serves(served) {
            self.release_keys();
        }
    }

    fn completion_done(&mut self, generation: u64, result: Result<Completion, String>) {
        self.effects.log_result(&result);
        self.dispatch(Event::Completed { generation, result });
    }

    fn model_ready(&mut self, status: Result<String, String>) {
        match status {
            Ok(name) => {
                log(&format!("ready ({name})"));
                self.model_changed(Model::Ready, OnboardingEvent::Ready);
            }
            Err(error) => {
                log(&format!("error: {error}"));
                self.model_changed(Model::Failed(error.clone()), OnboardingEvent::Failed(error));
            }
        }
    }

    fn model_preparing(&mut self, stage: Stage) {
        match stage {
            Stage::Downloading(progress) => self.model_changed(
                Model::Downloading {
                    percent: progress.percent(),
                },
                OnboardingEvent::Progress(progress.done, progress.total),
            ),
            Stage::Loading { file } => {
                log(&format!("loading {file}"));
                self.model_changed(Model::Loading, OnboardingEvent::Loading);
            }
        }
    }

    fn model_changed(&mut self, model: Model, event: OnboardingEvent) {
        self.presenter.set_model(model);
        self.refresh();
        self.presenter.onboard(event);
    }
}
