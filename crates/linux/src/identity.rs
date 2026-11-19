use atspi::{AtspiError, ObjectRefOwned};
use zbus::Connection;

use crate::process::{owner_pid, process_name};
use crate::window;

pub(crate) struct Identity {
    pub(crate) pid: u32,
    pub(crate) app: Option<String>,
    pub(crate) title: String,
}

impl Identity {
    pub(crate) async fn resolve(
        connection: &Connection,
        target: &ObjectRefOwned,
    ) -> Result<Self, AtspiError> {
        let pid = owner_pid(connection, target.name().ok_or(AtspiError::MissingName)?).await?;
        Ok(Self {
            pid,
            app: process_name(pid),
            title: window::title(connection, target).await.unwrap_or_default(),
        })
    }
}
