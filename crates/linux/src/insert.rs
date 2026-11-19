use std::time::{Duration, Instant};

use async_io::Timer;
use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::proxy::text::TextProxy;
use zbus::Connection;

use crate::error::{InsertError, Inserted};
use crate::focus::Focus;

const LANDING_WAIT: Duration = Duration::from_millis(200);
const LANDING_POLL: Duration = Duration::from_millis(20);

async fn landed(reading: &TextProxy<'_>, expected: i32) -> Result<bool, InsertError> {
    let deadline = Instant::now() + LANDING_WAIT;
    loop {
        if reading.character_count().await? == expected {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        Timer::after(LANDING_POLL).await;
    }
}

pub(crate) async fn insert(
    connection: &Connection,
    focus: &Focus,
    text: &str,
) -> Result<Inserted, InsertError> {
    let node = focus.target.as_accessible_proxy(connection).await?;
    let proxies = node.proxies().await?;
    let (Ok(editable), Ok(reading)) = (proxies.editable_text().await, proxies.text().await) else {
        return Err(InsertError::Unsupported);
    };
    let caret = reading.caret_offset().await?;
    let count = reading.character_count().await?;
    let length = i32::try_from(text.len()).map_err(|_| InsertError::Rejected)?;
    let characters = i32::try_from(text.chars().count()).map_err(|_| InsertError::Rejected)?;
    if !editable.insert_text(caret, text, length).await? {
        return Err(InsertError::Rejected);
    }
    if !landed(&reading, count.saturating_add(characters)).await? {
        return Ok(Inserted::Unconfirmed);
    }
    let _ = reading
        .set_caret_offset(caret.saturating_add(characters))
        .await;
    Ok(Inserted::Confirmed)
}
