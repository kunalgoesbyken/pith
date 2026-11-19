use std::hash::{DefaultHasher, Hash, Hasher};

use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::proxy::text::TextProxy;
use atspi::{AtspiError, ObjectRefOwned, Role, State};
use pith_core::{Config, FieldSnapshot, place};
use zbus::Connection;

use crate::process::{owner_pid, process_name};
use crate::{caret, style, window};

const AFTER_CHARS: i32 = 200;
const ASTRAL_FILLER: char = '\u{feff}';

fn field_id(target: &ObjectRefOwned) -> u64 {
    let mut hasher = DefaultHasher::new();
    target.hash(&mut hasher);
    hasher.finish()
}

async fn surrounding(
    text: &TextProxy<'_>,
    caret: i32,
    count: i32,
    lookback: usize,
) -> Result<(String, String), AtspiError> {
    let start = caret
        .saturating_sub(i32::try_from(lookback).unwrap_or(i32::MAX))
        .max(0);
    let end = caret.saturating_add(AFTER_CHARS).min(count);
    Ok((
        text.get_text(start, caret)
            .await?
            .replace(ASTRAL_FILLER, ""),
        text.get_text(caret, end).await?.replace(ASTRAL_FILLER, ""),
    ))
}

pub(crate) async fn read(
    connection: &Connection,
    target: &ObjectRefOwned,
    config: &Config,
) -> Result<Option<FieldSnapshot>, AtspiError> {
    let node = target.as_accessible_proxy(connection).await?;
    let role = node.get_role().await?;
    if !node.get_state().await?.contains(State::Editable) || role == Role::Terminal {
        return Ok(None);
    }
    let Ok(text) = node.proxies().await?.text().await else {
        return Ok(None);
    };
    let pid = owner_pid(connection, target.name().ok_or(AtspiError::MissingName)?).await?;
    if pid == std::process::id() {
        return Ok(None);
    }
    let app = process_name(pid);
    let blocked = app.as_deref().is_none_or(|name| config.is_blocked(name));
    let is_password = role == Role::PasswordText;
    let count = text.character_count().await?;
    let caret = text.caret_offset().await?;
    if caret < 0 || count < 0 {
        return Ok(None);
    }
    let caret = caret.min(count);
    let (before, after) = if is_password || blocked {
        Default::default()
    } else {
        surrounding(&text, caret, count, config.lookback_chars()).await?
    };
    Ok(Some(FieldSnapshot {
        field_id: field_id(target),
        app: app.unwrap_or_default(),
        window_title: window::title(connection, target).await?,
        before,
        after,
        has_selection: caret::has_selection(&text).await,
        is_password,
        composing: false,
        placement: place(caret::rect(&text, caret, count).await, None, &[]),
        style: style::read(&text, caret).await,
    }))
}
