use std::hash::{DefaultHasher, Hash, Hasher};

use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::proxy::text::TextProxy;
use atspi::{AtspiError, ObjectRefOwned, Role, State};
use pith_core::{
    AFTER_CHARS, Config, FieldParts, FieldSnapshot, LINE_BREAKS, Rect, Surrounding, is_redacted,
    place,
};
use zbus::Connection;

use crate::focus::Focus;
use crate::reading::Reading;
use crate::text::without_astral_fillers;
use crate::{caret, style, window};

const TEAR_RETRIES: usize = 2;

struct View {
    caret: i32,
    count: i32,
    surrounding: Option<Surrounding>,
}

pub(crate) fn field_id(target: &ObjectRefOwned) -> u64 {
    let mut hasher = DefaultHasher::new();
    target.hash(&mut hasher);
    hasher.finish()
}

async fn position(text: &TextProxy<'_>) -> Result<Option<(i32, i32)>, AtspiError> {
    let count = text.character_count().await?;
    let caret = text.caret_offset().await?;
    Ok((caret >= 0 && count >= 0).then_some((caret.min(count), count)))
}

async fn around_caret(
    text: &TextProxy<'_>,
    caret: i32,
    count: i32,
    lookback: usize,
) -> Result<(Surrounding, i32), AtspiError> {
    let start = caret
        .saturating_sub(i32::try_from(lookback).unwrap_or(i32::MAX))
        .max(0);
    let end = caret
        .saturating_add(i32::try_from(AFTER_CHARS).unwrap_or(i32::MAX))
        .min(count);
    let surrounding = Surrounding {
        before: without_astral_fillers(&text.get_text(start, caret).await?),
        after: without_astral_fillers(&text.get_text(caret, end).await?),
    };
    Ok((surrounding, text.caret_offset().await?))
}

async fn view(
    text: &TextProxy<'_>,
    redacted: bool,
    lookback: usize,
) -> Result<Option<View>, AtspiError> {
    let Some((mut caret, mut count)) = position(text).await? else {
        return Ok(None);
    };
    if redacted {
        return Ok(Some(View {
            caret,
            count,
            surrounding: None,
        }));
    }
    let mut retries = 0;
    loop {
        let (surrounding, settled) = around_caret(text, caret, count, lookback).await?;
        if settled == caret {
            return Ok(Some(View {
                caret,
                count,
                surrounding: Some(surrounding),
            }));
        }
        if retries == TEAR_RETRIES {
            return Ok(None);
        }
        retries += 1;
        let Some(moved) = position(text).await? else {
            return Ok(None);
        };
        (caret, count) = moved;
    }
}

pub(crate) async fn read(
    connection: &Connection,
    focus: &mut Focus,
    config: &Config,
    works: &[Rect],
) -> Result<Reading, AtspiError> {
    let target = focus.target.clone();
    let node = target.as_accessible_proxy(connection).await?;
    let role = node.get_role().await?;
    if !node.get_state().await?.contains(State::Editable) || role == Role::Terminal {
        return Ok(Reading::Nothing);
    }
    let Ok(text) = node.proxies().await?.text().await else {
        return Ok(Reading::Nothing);
    };
    let identity = focus.identity(connection).await?;
    if identity.pid == std::process::id() {
        return Ok(Reading::Nothing);
    }
    let Some(app) = identity.app.as_deref() else {
        return Ok(Reading::Nothing);
    };
    let is_password = role == Role::PasswordText;
    let redacted = is_redacted(config, Some(app), is_password);
    let Some(view) = view(&text, redacted, config.lookback_chars()).await? else {
        return Ok(Reading::Nothing);
    };
    let previous_breaks_line = view
        .surrounding
        .as_ref()
        .is_some_and(|text| text.before.ends_with(LINE_BREAKS));
    let rect = caret::rect(&text, view.caret, view.count, previous_breaks_line).await;
    let element = match rect {
        Some(_) => None,
        None => caret::element(&node).await,
    };
    Ok(Reading::Field(Box::new(FieldSnapshot::from_parts(
        config,
        FieldParts {
            field_id: field_id(&target),
            app: Some(app.to_string()),
            window_title: if redacted {
                String::new()
            } else {
                window::title(connection, &target).await.unwrap_or_default()
            },
            surrounding: view.surrounding,
            has_selection: caret::has_selection(&text).await,
            is_password,
            composing: false,
            language: None,
            placement: place(rect, element, works),
            style: style::read(&text, view.caret).await,
        },
    ))))
}
