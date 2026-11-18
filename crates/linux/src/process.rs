use std::path::PathBuf;

use atspi::AtspiError;
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::names::{BusName, UniqueName};

const PROC_ROOT: &str = "/proc";
const EXECUTABLE_LINK: &str = "exe";

pub(crate) async fn owner_pid(
    connection: &Connection,
    name: &UniqueName<'_>,
) -> Result<u32, AtspiError> {
    let bus = DBusProxy::new(connection).await?;
    Ok(bus
        .get_connection_unix_process_id(BusName::from(name.clone()))
        .await?)
}

pub(crate) fn process_name(pid: u32) -> Option<String> {
    let executable = std::fs::read_link(
        PathBuf::from(PROC_ROOT)
            .join(pid.to_string())
            .join(EXECUTABLE_LINK),
    )
    .ok()?;
    Some(executable.file_name()?.to_string_lossy().into_owned())
}
