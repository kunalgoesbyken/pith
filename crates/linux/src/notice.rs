use std::collections::HashMap;

use futures_lite::future::block_on;
use pith_core::log;
use zbus::Connection;
use zbus::zvariant::Value;

const APP: &str = "Pith";
const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const NOTIFY: &str = "Notify";
const NEW_NOTICE: u32 = 0;
const SERVER_TIMEOUT: i32 = -1;

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

pub fn notify(body: String) {
    std::thread::spawn(move || {
        if let Err(error) = block_on(send(&body)) {
            log(&format!("notice not shown: {error}"));
        }
    });
}
