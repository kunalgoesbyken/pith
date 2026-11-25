use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, sync_channel};

use futures_lite::future::block_on;
use pith_core::log;
use zbus::Connection;
use zbus::zvariant::Value;

use crate::bounded::{STARTUP_DEADLINE, within};

const APP: &str = "Pith";
const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const NOTIFY: &str = "Notify";
const NEW_NOTICE: u32 = 0;
const SERVER_TIMEOUT: i32 = -1;
const WAITING: usize = 4;

static NOTICES: OnceLock<SyncSender<String>> = OnceLock::new();

fn escaped(body: &str) -> String {
    body.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn send(body: &str) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let message = (
        APP,
        NEW_NOTICE,
        "",
        APP,
        body,
        Vec::<&str>::new(),
        HashMap::<&str, Value>::new(),
        SERVER_TIMEOUT,
    );
    connection
        .call_method(Some(SERVICE), PATH, Some(SERVICE), NOTIFY, &message)
        .await?;
    Ok(())
}

fn deliver(body: &str) {
    match block_on(within(STARTUP_DEADLINE, send(&escaped(body)))) {
        Some(Ok(())) => {}
        Some(Err(error)) => log(&format!("notice not shown: {error}")),
        None => log("notice not shown: the notification server did not answer"),
    }
}

fn worker() -> SyncSender<String> {
    let (sender, receiver) = sync_channel::<String>(WAITING);
    std::thread::spawn(move || {
        for body in receiver {
            deliver(&body);
        }
    });
    sender
}

pub fn notify(body: String) {
    if NOTICES.get_or_init(worker).try_send(body).is_err() {
        log("notice not shown: too many are waiting");
    }
}
