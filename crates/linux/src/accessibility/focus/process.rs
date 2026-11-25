use std::path::{Path, PathBuf};

use atspi::AtspiError;
use std::sync::Mutex;

use pith_core::{entry_of, is_generic_entry, is_runtime, is_unlisted_id, log};
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::names::{BusName, UniqueName};

const PROC_ROOT: &str = "/proc";
const EXECUTABLE_LINK: &str = "exe";
const COMMAND_LINE: &str = "cmdline";
const ENVIRONMENT: &str = "environ";
const REPLACED_SUFFIX: &str = " (deleted)";
const FLATPAK_VARIABLE: &str = "FLATPAK_ID";
const SNAP_VARIABLE: &str = "SNAP_NAME";
const SNAP_PREFIX: &str = "snap.";

static ANNOUNCED: Mutex<Vec<String>> = Mutex::new(Vec::new());
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

fn sandbox_value(environment: &[String], command: &[String], variable: &str) -> Option<String> {
    let from_environment = environment
        .iter()
        .find_map(|entry| entry.strip_prefix(variable)?.strip_prefix('='));
    let from_command = command
        .windows(3)
        .find(|window| window[0] == SET_ENVIRONMENT_FLAG && window[1] == variable)
        .map(|window| window[2].as_str());
    from_environment
        .or(from_command)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sandbox_name(pid: u32) -> Option<String> {
    let environment = fields(pid, ENVIRONMENT);
    let command = fields(pid, COMMAND_LINE);
    sandbox_value(&environment, &command, FLATPAK_VARIABLE).or_else(|| {
        sandbox_value(&environment, &command, SNAP_VARIABLE)
            .map(|name| format!("{SNAP_PREFIX}{name}"))
    })
}

fn announce(app: &str) {
    let mut seen = ANNOUNCED.lock().unwrap_or_else(|error| error.into_inner());
    if is_unlisted_id(app) && !seen.iter().any(|known| known == app) {
        seen.push(app.to_string());
        log(&format!(
            "{app} is not a known app, so it is off unless allowed_apps lists \"{app}\""
        ));
    }
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
    let stem = entry.file_stem()?.to_string_lossy().into_owned();
    (!is_generic_entry(&stem)).then_some(stem)
}

pub(crate) fn process_name(pid: u32) -> Option<String> {
    let executable = executable_name(pid)?;
    if !is_runtime(&executable) {
        return Some(executable);
    }
    let name = sandbox_name(pid).or_else(|| hosted_name(pid, &executable))?;
    announce(&name);
    Some(name)
}
