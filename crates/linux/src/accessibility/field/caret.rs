use atspi::CoordType;
use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::proxy::text::TextProxy;
use pith_core::Rect;

const CARET_WIDTH: i32 = 1;

async fn glyph(text: &TextProxy<'_>, offset: i32) -> Option<Rect> {
    let (x, y, width, height) = text
        .get_character_extents(offset, CoordType::Screen)
        .await
        .ok()?;
    (height > 0).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}

pub(crate) async fn rect(
    text: &TextProxy<'_>,
    caret: i32,
    count: i32,
    previous_breaks_line: bool,
) -> Option<Rect> {
    if caret > 0
        && !previous_breaks_line
        && let Some(last) = glyph(text, caret - 1).await
    {
        return Some(Rect {
            x: last.x.saturating_add(last.width),
            width: CARET_WIDTH,
            ..last
        });
    }
    if caret < count {
        let next = glyph(text, caret).await?;
        return Some(Rect {
            width: CARET_WIDTH,
            ..next
        });
    }
    None
}

pub(crate) async fn has_selection(text: &TextProxy<'_>) -> bool {
    if text.get_n_selections().await.unwrap_or(0) < 1 {
        return false;
    }
    text.get_selection(0)
        .await
        .is_ok_and(|(start, end)| start != end)
}

pub(crate) async fn element(node: &AccessibleProxy<'_>) -> Option<Rect> {
    let component = node.proxies().await.ok()?.component().await.ok()?;
    let screen = component.get_extents(CoordType::Screen).await.ok()?;
    let window = component.get_extents(CoordType::Window).await.ok()?;
    let (x, y, width, height) = screen;
    (screen != window && width > 0 && height > 0).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}
