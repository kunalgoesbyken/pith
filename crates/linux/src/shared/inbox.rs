use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Completion, TrayCommand};
use pith_infer::Stage;

use crate::accessibility::Report;
use crate::error::ReaderError;
use crate::host::Tray;
use crate::x11::{DisplayError, Stroke};

pub enum Inbound {
    Report(Result<Report, ReaderError>),
    Done(u64, Result<Completion, String>),
    Ready(Result<String, String>),
    Preparing(Stage),
    Command(TrayCommand),
    Tray(Tray),
    NoTray,
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
