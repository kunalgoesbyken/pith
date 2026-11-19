use std::fmt;
use std::sync::Arc;

use x11rb::connection::Connection;
use x11rb::errors::{ConnectError, ConnectionError, ReplyError, ReplyOrIdError};
use x11rb::protocol::xproto::{ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

const POINTER_ROOT: Window = 1;

pub struct Display {
    pub(crate) connection: RustConnection,
    pub(crate) root: Window,
    pub(crate) screen: usize,
}

#[derive(Debug)]
pub enum DisplayError {
    Connect(ConnectError),
    Request(ReplyOrIdError),
    Missing(&'static str),
}

impl fmt::Display for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(error) => write!(f, "x11 connection: {error}"),
            Self::Request(error) => write!(f, "x11 request: {error}"),
            Self::Missing(extension) => write!(f, "the x server has no {extension} extension"),
        }
    }
}

impl std::error::Error for DisplayError {}

impl From<ConnectError> for DisplayError {
    fn from(error: ConnectError) -> Self {
        Self::Connect(error)
    }
}

impl From<ConnectionError> for DisplayError {
    fn from(error: ConnectionError) -> Self {
        Self::Request(error.into())
    }
}

impl From<ReplyError> for DisplayError {
    fn from(error: ReplyError) -> Self {
        Self::Request(error.into())
    }
}

impl From<ReplyOrIdError> for DisplayError {
    fn from(error: ReplyOrIdError) -> Self {
        Self::Request(error)
    }
}

impl Display {
    pub fn connect() -> Result<Arc<Self>, DisplayError> {
        let (connection, screen) = RustConnection::connect(None)?;
        let root = connection.setup().roots[screen].root;
        Ok(Arc::new(Self {
            connection,
            root,
            screen,
        }))
    }

    pub(crate) fn input_focus(&self) -> Option<Window> {
        let focus = self.connection.get_input_focus().ok()?.reply().ok()?.focus;
        (focus != x11rb::NONE && focus != POINTER_ROOT).then_some(focus)
    }
}
