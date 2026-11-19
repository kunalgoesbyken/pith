use std::sync::Arc;
use std::thread::JoinHandle;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::Mapping;

use super::desktop::Desktop;
use super::display::{Display, DisplayError};
use super::grab::{Grabs, Stroke};
use crate::held::Held;

pub fn listen(
    display: Arc<Display>,
    grabs: Grabs,
    desktop: Desktop,
    held: Held,
    on_key: impl Fn(Stroke) + Send + 'static,
    on_closed: impl Fn(DisplayError) + Send + 'static,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        loop {
            match display.connection.wait_for_event() {
                Ok(Event::KeyPress(press)) => {
                    held.begin();
                    on_key(Stroke {
                        keycode: press.detail,
                        state: u16::from(press.state),
                        time: press.time,
                    });
                }
                Ok(Event::MappingNotify(change))
                    if change.request == Mapping::KEYBOARD
                        || change.request == Mapping::MODIFIER =>
                {
                    grabs.remap();
                }
                Ok(event) => desktop.observe(&event),
                Err(error) => {
                    on_closed(error.into());
                    return;
                }
            }
        }
    })
}
