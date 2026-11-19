use std::path::PathBuf;

use atspi::AtspiError;
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::names::{BusName, UniqueName};

const PROC_ROOT: &str = "/proc";
const EXECUTABLE_LINK: &str = "exe";
const REPLACED_SUFFIX: &str = " (deleted)";

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
    let name = executable.file_name()?.to_string_lossy();
    Some(
        name.strip_suffix(REPLACED_SUFFIX)
            .unwrap_or(&name)
            .to_string(),
    )
}
