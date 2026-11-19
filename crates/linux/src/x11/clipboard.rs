use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pith_core::log;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt, EventMask, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, PropMode,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
};
use x11rb::protocol::xtest::{ConnectionExt as _, X11_EXTENSION_NAME as XTEST};
use x11rb::wrapper::ConnectionExt as _;

use super::display::{Display, DisplayError};
use super::keymap::Keyboard;

const FETCH_WAIT: Duration = Duration::from_millis(300);
const FIRST_REQUEST_WAIT: Duration = Duration::from_millis(1000);
const REQUEST_QUIET: Duration = Duration::from_millis(300);
const FOCUS_SETTLE: Duration = Duration::from_millis(150);
const NO_TIME: u32 = 0;
const MAX_REPLY_WORDS: u32 = u32::MAX / 4;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD,
        UTF8_STRING,
        TARGETS,
        PITH_CLIPBOARD,
    }
}

struct Shared {
    display: Arc<Display>,
    window: Window,
    atoms: Atoms,
    offer: Mutex<Option<String>>,
}

struct Replies {
    answers: Receiver<SelectionNotifyEvent>,
    served: Receiver<()>,
}

struct Request {
    text: String,
    target: Window,
}

enum Previous {
    Empty,
    Text(String),
    Unreadable,
}

pub struct Clipboard {
    shared: Arc<Shared>,
    pastes: Sender<Request>,
}

impl Shared {
    fn offer(&self) -> Option<String> {
        self.offer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_offer(&self, text: Option<String>) {
        *self.offer.lock().unwrap_or_else(PoisonError::into_inner) = text;
    }

    fn own(&self, owner: Window) -> Result<(), DisplayError> {
        self.display
            .connection
            .set_selection_owner(owner, self.atoms.CLIPBOARD, NO_TIME)?;
        self.display.connection.flush()?;
        Ok(())
    }

    fn answer(&self, request: &SelectionRequestEvent) -> Result<bool, DisplayError> {
        let connection = &self.display.connection;
        let property = if request.property == x11rb::NONE {
            request.target
        } else {
            request.property
        };
        let served = match (request.target, self.offer()) {
            (target, _) if target == self.atoms.TARGETS => {
                let targets = [self.atoms.TARGETS, self.atoms.UTF8_STRING];
                connection.change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )?;
                false
            }
            (target, Some(text)) if target == self.atoms.UTF8_STRING => {
                connection.change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    self.atoms.UTF8_STRING,
                    text.as_bytes(),
                )?;
                true
            }
            _ => {
                respond(self, request, x11rb::NONE)?;
                return Ok(false);
            }
        };
        respond(self, request, property)?;
        Ok(served)
    }
}

fn respond(
    shared: &Shared,
    request: &SelectionRequestEvent,
    property: Atom,
) -> Result<(), DisplayError> {
    let event = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: request.time,
        requestor: request.requestor,
        selection: request.selection,
        target: request.target,
        property,
    };
    shared
        .display
        .connection
        .send_event(false, request.requestor, EventMask::NO_EVENT, event)?;
    shared.display.connection.flush()?;
    Ok(())
}

fn listen(
    shared: Arc<Shared>,
    answers: Sender<SelectionNotifyEvent>,
    served: Sender<()>,
    on_closed: impl Fn(DisplayError),
) {
    loop {
        match shared.display.connection.wait_for_event() {
            Ok(Event::SelectionRequest(request)) => match shared.answer(&request) {
                Ok(true) => {
                    let _ = served.send(());
                }
                Ok(false) => {}
                Err(error) => log(&format!("clipboard reply failed: {error}")),
            },
            Ok(Event::SelectionNotify(answer)) => {
                let _ = answers.send(answer);
            }
            Ok(Event::SelectionClear(_)) => shared.set_offer(None),
            Ok(_) => {}
            Err(error) => {
                on_closed(error.into());
                return;
            }
        }
    }
}

fn previous(
    shared: &Shared,
    answers: &Receiver<SelectionNotifyEvent>,
) -> Result<Previous, DisplayError> {
    while answers.try_recv().is_ok() {}
    let connection = &shared.display.connection;
    let owner = connection
        .get_selection_owner(shared.atoms.CLIPBOARD)?
        .reply()?
        .owner;
    if owner == x11rb::NONE {
        return Ok(Previous::Empty);
    }
    connection.convert_selection(
        shared.window,
        shared.atoms.CLIPBOARD,
        shared.atoms.UTF8_STRING,
        shared.atoms.PITH_CLIPBOARD,
        NO_TIME,
    )?;
    connection.flush()?;
    let Ok(answer) = answers.recv_timeout(FETCH_WAIT) else {
        return Ok(Previous::Unreadable);
    };
    if answer.property == x11rb::NONE {
        return Ok(Previous::Unreadable);
    }
    let reply = connection
        .get_property(
            true,
            shared.window,
            shared.atoms.PITH_CLIPBOARD,
            AtomEnum::ANY,
            0,
            MAX_REPLY_WORDS,
        )?
        .reply()?;
    let complete = reply.type_ == shared.atoms.UTF8_STRING && reply.bytes_after == 0;
    Ok(match String::from_utf8(reply.value) {
        Ok(text) if complete => Previous::Text(text),
        _ => Previous::Unreadable,
    })
}

fn press_paste(shared: &Shared) -> Result<(), DisplayError> {
    let connection = &shared.display.connection;
    connection
        .extension_information(XTEST)?
        .ok_or(DisplayError::Missing("XTEST"))?;
    let keyboard = Keyboard::read(&shared.display)?;
    let (Some(control), Some(letter)) = (keyboard.control_keycode(), keyboard.paste_keycode())
    else {
        return Err(DisplayError::Missing("a Control or V key"));
    };
    for (event, keycode) in [
        (KEY_PRESS_EVENT, control),
        (KEY_PRESS_EVENT, letter),
        (KEY_RELEASE_EVENT, letter),
        (KEY_RELEASE_EVENT, control),
    ] {
        connection.xtest_fake_input(event, keycode, NO_TIME, shared.display.root, 0, 0, 0)?;
    }
    connection.flush()?;
    Ok(())
}

fn deliver(shared: &Shared, replies: &Replies, target: Window) -> Result<(), DisplayError> {
    std::thread::sleep(FOCUS_SETTLE);
    if shared.display.input_focus() != Some(target) {
        log("the focus moved before the paste, nothing was pasted");
        return Ok(());
    }
    while replies.served.try_recv().is_ok() {}
    press_paste(shared)?;
    if replies.served.recv_timeout(FIRST_REQUEST_WAIT).is_ok() {
        while replies.served.recv_timeout(REQUEST_QUIET).is_ok() {}
    }
    Ok(())
}

fn restore(shared: &Shared, before: Option<String>) -> Result<(), DisplayError> {
    match before {
        Some(text) => shared.set_offer(Some(text)),
        None => {
            shared.set_offer(None);
            shared.own(x11rb::NONE)?;
        }
    }
    Ok(())
}

fn paste(shared: &Shared, replies: &Replies, request: Request) -> Result<(), DisplayError> {
    let before = match previous(shared, &replies.answers)? {
        Previous::Unreadable => {
            log("the clipboard holds data Pith cannot restore, nothing was pasted");
            return Ok(());
        }
        Previous::Empty => None,
        Previous::Text(text) => Some(text),
    };
    shared.set_offer(Some(request.text));
    shared.own(shared.window)?;
    let delivered = deliver(shared, replies, request.target);
    restore(shared, before)?;
    delivered
}

impl Clipboard {
    pub fn spawn(on_closed: impl Fn(DisplayError) + Send + 'static) -> Result<Self, DisplayError> {
        let display = Display::connect()?;
        let connection = &display.connection;
        let atoms = Atoms::new(connection)?.reply()?;
        let window = connection.generate_id()?;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            display.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &Default::default(),
        )?;
        connection.flush()?;
        let shared = Arc::new(Shared {
            display,
            window,
            atoms,
            offer: Mutex::new(None),
        });
        let (answers_in, answers) = channel();
        let (served_in, served) = channel();
        let listener = shared.clone();
        std::thread::spawn(move || listen(listener, answers_in, served_in, on_closed));
        let (pastes, requested) = channel::<Request>();
        let worker = shared.clone();
        std::thread::spawn(move || {
            let replies = Replies { answers, served };
            for request in requested {
                if let Err(error) = paste(&worker, &replies, request) {
                    log(&format!("paste failed: {error}"));
                }
            }
        });
        Ok(Self { shared, pastes })
    }

    pub fn paste(&self, text: String) {
        match self.shared.display.input_focus() {
            Some(target) => {
                let _ = self.pastes.send(Request { text, target });
            }
            None => log("no window has the focus, nothing was pasted"),
        }
    }
}
