use std::path::{Path, PathBuf};

use atspi::AtspiError;
use pith_core::{entry_of, is_runtime};
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::names::{BusName, UniqueName};

const PROC_ROOT: &str = "/proc";
const EXECUTABLE_LINK: &str = "exe";
const COMMAND_LINE: &str = "cmdline";
const ENVIRONMENT: &str = "environ";
const REPLACED_SUFFIX: &str = " (deleted)";
const SANDBOX_VARIABLES: [&str; 2] = ["FLATPAK_ID", "SNAP_NAME"];
const SET_ENVIRONMENT_FLAG: &str = "--setenv";
const UNNAMED_ENTRY_EXTENSION: &str = "asar";

pub(crate) async fn owner_pid(
    connection: &Connection,
    name: &UniqueName<'_>,
) -> Result<u32, AtspiError> {
    let bus = DBusProxy::new(connection).await?;
    Ok(bus
        .get_connection_unix_process_id(BusName::from(name.clone()))
        .await?)
}

fn proc_path(pid: u32, entry: &str) -> PathBuf {
    PathBuf::from(PROC_ROOT).join(pid.to_string()).join(entry)
}

fn fields(pid: u32, entry: &str) -> Vec<String> {
    std::fs::read(proc_path(pid, entry))
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter(|field| !field.is_empty())
                .map(|field| String::from_utf8_lossy(field).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn executable_name(pid: u32) -> Option<String> {
    let executable = std::fs::read_link(proc_path(pid, EXECUTABLE_LINK)).ok()?;
    let name = executable.file_name()?.to_string_lossy();
    Some(
        name.strip_suffix(REPLACED_SUFFIX)
            .unwrap_or(&name)
            .to_string(),
    )
}

fn sandbox_name(pid: u32) -> Option<String> {
    let environment = fields(pid, ENVIRONMENT);
    let command = fields(pid, COMMAND_LINE);
    SANDBOX_VARIABLES.iter().find_map(|variable| {
        let from_environment = environment
            .iter()
            .find_map(|entry| entry.strip_prefix(variable)?.strip_prefix('='));
        let from_command = command
            .windows(3)
            .find(|window| window[0] == SET_ENVIRONMENT_FLAG && window[1] == *variable)
            .map(|window| window[2].as_str());
        from_environment
            .or(from_command)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
    })
}

fn hosted_name(pid: u32, runtime: &str) -> Option<String> {
    let command = fields(pid, COMMAND_LINE);
    let entry = Path::new(entry_of(runtime, command.get(1..)?)?);
    if entry
        .extension()
        .is_some_and(|extension| extension == UNNAMED_ENTRY_EXTENSION)
    {
        return None;
    }
    Some(entry.file_stem()?.to_string_lossy().into_owned())
}

pub(crate) fn process_name(pid: u32) -> Option<String> {
    let executable = executable_name(pid)?;
    if !is_runtime(&executable) {
        return Some(executable);
    }
    sandbox_name(pid).or_else(|| hosted_name(pid, &executable))
}
