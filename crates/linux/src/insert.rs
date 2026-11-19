use std::time::{Duration, Instant};

use async_io::Timer;
use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::proxy::text::TextProxy;
use zbus::Connection;

use crate::error::{InsertError, Inserted, Target};
use crate::field::field_id;
use crate::focus::Focus;

const LANDING_WAIT: Duration = Duration::from_millis(200);
const LANDING_POLL: Duration = Duration::from_millis(20);

async fn landed(reading: &TextProxy<'_>, expected: i32) -> Option<bool> {
    let deadline = Instant::now() + LANDING_WAIT;
    loop {
        if reading.character_count().await.ok()? == expected {
            return Some(true);
        }
        if Instant::now() >= deadline {
            return Some(false);
        }
        Timer::after(LANDING_POLL).await;
    }
}

async fn is_target(connection: &Connection, focus: &mut Focus, target: &Target) -> bool {
    let id = field_id(&focus.target);
    let identity = focus.identity(connection).await.ok();
    id == target.field_id
        && identity.is_some_and(|identity| identity.app.as_deref() == Some(target.app.as_str()))
}

pub(crate) async fn insert(
    connection: &Connection,
    focus: &mut Focus,
    text: &str,
    target: &Target,
) -> Result<Inserted, InsertError> {
    if !is_target(connection, focus, target).await {
        return Err(InsertError::NoField);
    }
    let node = focus
        .target
        .as_accessible_proxy(connection)
        .await
        .map_err(|_| InsertError::Unsupported)?;
    let proxies = node.proxies().await.map_err(|_| InsertError::Unsupported)?;
    let (Ok(editable), Ok(reading)) = (proxies.editable_text().await, proxies.text().await) else {
        return Err(InsertError::Unsupported);
    };
    let (Ok(caret), Ok(count)) = (
        reading.caret_offset().await,
        reading.character_count().await,
    ) else {
        return Err(InsertError::Unsupported);
    };
    let (Ok(length), Ok(characters)) = (
        i32::try_from(text.len()),
        i32::try_from(text.chars().count()),
    ) else {
        return Err(InsertError::Rejected);
    };
    match editable.insert_text(caret, text, length).await {
        Ok(true) => {}
        Ok(false) => return Err(InsertError::Rejected),
        Err(error) => return Err(InsertError::Bus(error.into())),
    }
    if landed(&reading, count.saturating_add(characters)).await != Some(true) {
        return Ok(Inserted::Unconfirmed);
    }
    let _ = reading
        .set_caret_offset(caret.saturating_add(characters))
        .await;
    Ok(Inserted::Confirmed)
}
