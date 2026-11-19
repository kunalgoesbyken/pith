use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
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
use crate::current::Current;
use crate::error::Target;

const FETCH_WAIT: Duration = Duration::from_millis(300);
const SAVE_WAIT: Duration = Duration::from_millis(2000);
const FIRST_REQUEST_WAIT: Duration = Duration::from_millis(1000);
const REQUEST_QUIET: Duration = Duration::from_millis(300);
const FOCUS_SETTLE: Duration = Duration::from_millis(150);
const NO_TIME: u32 = 0;
const ATOM_BYTES: usize = 4;
const MAX_REPLY_WORDS: u32 = u32::MAX / 4;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD,
        UTF8_STRING,
        TARGETS,
        INCR,
        PITH_CLIPBOARD,
        CLIPBOARD_MANAGER,
        SAVE_TARGETS,
        TEXT_PLAIN_UTF8: b"text/plain;charset=utf-8",
        TEXT_HTML: b"text/html",
        TEXT_PLAIN: b"text/plain",
        TEXT,
        COMPOUND_TEXT,
        TIMESTAMP,
        MULTIPLE,
        DELETE,
        INSERT_SELECTION,
        INSERT_PROPERTY,
    }
}

type Formats = Vec<(Atom, Vec<u8>)>;

struct Shared {
    display: Arc<Display>,
    window: Window,
    atoms: Atoms,
    offer: Mutex<Option<Formats>>,
    owned: AtomicBool,
    current: Current,
    unpasted: Box<dyn Fn() + Send + Sync>,
}

struct Replies {
    answers: Receiver<SelectionNotifyEvent>,
    served: Receiver<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteTarget(Window);

struct Request {
    text: String,
    window: PasteTarget,
    field: Target,
}

enum Job {
    Paste(Request),
    Save,
}

enum Previous {
    Empty,
    Saved(Formats),
    Unreadable,
}

pub struct Clipboard {
    shared: Arc<Shared>,
    jobs: Sender<Job>,
    worker: JoinHandle<()>,
}

impl Shared {
    fn offer(&self) -> Option<Formats> {
        self.offer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_offer(&self, formats: Option<Formats>) {
        *self.offer.lock().unwrap_or_else(PoisonError::into_inner) = formats;
    }

    fn owns(&self) -> bool {
        self.owned.load(Ordering::SeqCst)
    }

    fn own(&self, owner: Window) -> Result<(), DisplayError> {
        self.display
            .connection
            .set_selection_owner(owner, self.atoms.CLIPBOARD, NO_TIME)?;
        self.display.connection.flush()?;
        self.owned.store(owner != x11rb::NONE, Ordering::SeqCst);
        Ok(())
    }

    fn is_saved_or_protocol(&self, atom: Atom) -> bool {
        let atoms = &self.atoms;
        atom == u32::from(AtomEnum::STRING)
            || [
                atoms.UTF8_STRING,
                atoms.TEXT_PLAIN_UTF8,
                atoms.TEXT_HTML,
                atoms.TEXT_PLAIN,
                atoms.TEXT,
                atoms.COMPOUND_TEXT,
                atoms.TARGETS,
                atoms.TIMESTAMP,
                atoms.MULTIPLE,
                atoms.SAVE_TARGETS,
                atoms.DELETE,
                atoms.INSERT_SELECTION,
                atoms.INSERT_PROPERTY,
            ]
            .contains(&atom)
    }

    fn text_formats(&self, text: &str) -> Formats {
        [self.atoms.UTF8_STRING, self.atoms.TEXT_PLAIN_UTF8]
            .into_iter()
            .map(|atom| (atom, text.as_bytes().to_vec()))
            .collect()
    }

    fn answer(&self, request: &SelectionRequestEvent) -> Result<bool, DisplayError> {
        let connection = &self.display.connection;
        let property = if request.property == x11rb::NONE {
            request.target
        } else {
            request.property
        };
        let Some(formats) = self.offer() else {
            respond(self, request, x11rb::NONE)?;
            return Ok(false);
        };
        if request.target == self.atoms.TARGETS {
            let targets: Vec<Atom> = std::iter::once(self.atoms.TARGETS)
                .chain(formats.iter().map(|(atom, _)| *atom))
                .collect();
            connection.change_property32(
                PropMode::REPLACE,
                request.requestor,
                property,
                AtomEnum::ATOM,
                &targets,
            )?;
            respond(self, request, property)?;
            return Ok(false);
        }
        let Some((_, data)) = formats.iter().find(|(atom, _)| *atom == request.target) else {
            respond(self, request, x11rb::NONE)?;
            return Ok(false);
        };
        connection.change_property8(
            PropMode::REPLACE,
            request.requestor,
            property,
            request.target,
            data,
        )?;
        respond(self, request, property)?;
        Ok(true)
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
            Ok(Event::SelectionClear(_)) => {
                shared.owned.store(false, Ordering::SeqCst);
                shared.set_offer(None);
            }
            Ok(_) => {}
            Err(error) => {
                on_closed(error.into());
                return;
            }
        }
    }
}

fn fetch(
    shared: &Shared,
    answers: &Receiver<SelectionNotifyEvent>,
    target: Atom,
) -> Result<Option<(Atom, Vec<u8>)>, DisplayError> {
    let connection = &shared.display.connection;
    connection.convert_selection(
        shared.window,
        shared.atoms.CLIPBOARD,
        target,
        shared.atoms.PITH_CLIPBOARD,
        NO_TIME,
    )?;
    connection.flush()?;
    let Ok(answer) = answers.recv_timeout(FETCH_WAIT) else {
        return Ok(None);
    };
    if answer.property == x11rb::NONE {
        return Ok(None);
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
    let complete = reply.type_ != shared.atoms.INCR && reply.bytes_after == 0;
    Ok(complete.then_some((reply.type_, reply.value)))
}

fn previous(
    shared: &Shared,
    answers: &Receiver<SelectionNotifyEvent>,
) -> Result<Previous, DisplayError> {
    while answers.try_recv().is_ok() {}
    let owner = shared
        .display
        .connection
        .get_selection_owner(shared.atoms.CLIPBOARD)?
        .reply()?
        .owner;
    if owner == x11rb::NONE {
        return Ok(Previous::Empty);
    }
    let Some((_, listing)) = fetch(shared, answers, shared.atoms.TARGETS)? else {
        return Ok(Previous::Unreadable);
    };
    let offered: Vec<Atom> = listing
        .as_chunks::<ATOM_BYTES>()
        .0
        .iter()
        .map(|bytes| Atom::from_ne_bytes(*bytes))
        .collect();
    if !offered
        .iter()
        .all(|atom| shared.is_saved_or_protocol(*atom))
    {
        return Ok(Previous::Unreadable);
    }
    let wanted: Vec<Atom> = [
        shared.atoms.UTF8_STRING,
        shared.atoms.TEXT_PLAIN_UTF8,
        shared.atoms.TEXT_HTML,
    ]
    .into_iter()
    .filter(|atom| offered.contains(atom))
    .collect();
    if wanted.is_empty() {
        return Ok(Previous::Unreadable);
    }
    let mut saved = Formats::new();
    for target in wanted {
        match fetch(shared, answers, target)? {
            Some((_, data)) => saved.push((target, data)),
            None => return Ok(Previous::Unreadable),
        }
    }
    Ok(Previous::Saved(saved))
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

fn deliver(shared: &Shared, replies: &Replies, request: &Request) -> Result<bool, DisplayError> {
    std::thread::sleep(FOCUS_SETTLE);
    if shared.display.input_focus() != Some(request.window.0)
        || shared.current.get().as_ref() != Some(&request.field)
    {
        log("the focus moved before the paste, nothing was pasted");
        return Ok(false);
    }
    while replies.served.try_recv().is_ok() {}
    press_paste(shared)?;
    if replies.served.recv_timeout(FIRST_REQUEST_WAIT).is_err() {
        log("the application did not ask for the pasted text in time, nothing was pasted");
        return Ok(false);
    }
    while replies.served.recv_timeout(REQUEST_QUIET).is_ok() {}
    Ok(true)
}

fn restore(shared: &Shared, before: Option<Formats>) -> Result<(), DisplayError> {
    if !shared.owns() {
        return Ok(());
    }
    match before {
        Some(formats) => shared.set_offer(Some(formats)),
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
            (shared.unpasted)();
            return Ok(());
        }
        Previous::Empty => None,
        Previous::Saved(formats) => Some(formats),
    };
    shared.set_offer(Some(shared.text_formats(&request.text)));
    shared.own(shared.window)?;
    let delivered = deliver(shared, replies, &request);
    restore(shared, before)?;
    if !matches!(delivered, Ok(true)) {
        (shared.unpasted)();
    }
    delivered.map(drop)
}

fn save(shared: &Shared, replies: &Replies) -> Result<(), DisplayError> {
    let connection = &shared.display.connection;
    if !shared.owns() || shared.offer().is_none() {
        return Ok(());
    }
    let manager = connection
        .get_selection_owner(shared.atoms.CLIPBOARD_MANAGER)?
        .reply()?
        .owner;
    if manager == x11rb::NONE {
        return Ok(());
    }
    while replies.answers.try_recv().is_ok() {}
    connection.convert_selection(
        shared.window,
        shared.atoms.CLIPBOARD_MANAGER,
        shared.atoms.SAVE_TARGETS,
        shared.atoms.PITH_CLIPBOARD,
        NO_TIME,
    )?;
    connection.flush()?;
    let _ = replies.answers.recv_timeout(SAVE_WAIT);
    Ok(())
}

impl Clipboard {
    pub fn spawn(
        current: Current,
        on_unpasted: impl Fn() + Send + Sync + 'static,
        on_closed: impl Fn(DisplayError) + Send + 'static,
    ) -> Result<Self, DisplayError> {
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
            owned: AtomicBool::new(false),
            current,
            unpasted: Box::new(on_unpasted),
        });
        let (answers_in, answers) = channel();
        let (served_in, served) = channel();
        let listener = shared.clone();
        std::thread::spawn(move || listen(listener, answers_in, served_in, on_closed));
        let (jobs, requested) = channel::<Job>();
        let working = shared.clone();
        let worker = std::thread::spawn(move || {
            let replies = Replies { answers, served };
            for job in requested {
                let outcome = match job {
                    Job::Paste(request) => paste(&working, &replies, request),
                    Job::Save => save(&working, &replies),
                };
                if let Err(error) = outcome {
                    log(&format!("clipboard job failed: {error}"));
                }
            }
        });
        Ok(Self {
            shared,
            jobs,
            worker,
        })
    }

    pub fn focus(&self) -> Option<PasteTarget> {
        self.shared.display.input_focus().map(PasteTarget)
    }

    pub fn paste(&self, text: String, window: PasteTarget, field: Target) {
        let _ = self.jobs.send(Job::Paste(Request {
            text,
            window,
            field,
        }));
    }

    pub fn finish(self) {
        let Self { jobs, worker, .. } = self;
        let _ = jobs.send(Job::Save);
        drop(jobs);
        let _ = worker.join();
    }
}
