use pith_core::{Action, CompletionRequest};

pub(crate) fn trace(action: &Action) {
    if std::env::var("PITH_E2E_TRACE").is_ok() {
        println!(
            "[TRACE] {}",
            format!("{action:?}").chars().take(200).collect::<String>()
        );
    }
}

pub(crate) fn shown_text(action: &Action) -> String {
    match action {
        Action::Show { cuts, .. } => cuts.first().cloned().unwrap_or_default(),
        _ => String::new(),
    }
}

pub(crate) fn is_show(action: &Action) -> bool {
    matches!(action, Action::Show { .. })
}

pub(crate) fn is_hide(action: &Action) -> bool {
    matches!(action, Action::Hide)
}

pub(crate) fn is_auto_paused(action: &Action) -> bool {
    matches!(action, Action::AutoPaused(apps) if !apps.is_empty())
}

pub(crate) fn is_request(action: &Action) -> bool {
    matches!(action, Action::StartCompletion { .. })
}

pub(crate) fn inserted(seen: &[Action]) -> Vec<String> {
    seen.iter()
        .filter_map(|a| match a {
            Action::Insert(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

pub(crate) fn last_show(seen: &[Action]) -> Option<String> {
    seen.iter().rev().find(|a| is_show(a)).map(shown_text)
}

pub(crate) fn requests(seen: &[Action]) -> Vec<&CompletionRequest> {
    seen.iter()
        .filter_map(|a| match a {
            Action::StartCompletion { request, .. } => Some(request),
            _ => None,
        })
        .collect()
}

pub(crate) fn visible_text(seen: &[Action]) -> Option<String> {
    seen.iter()
        .rev()
        .find(|action| is_show(action) || is_hide(action))
        .filter(|action| is_show(action))
        .map(shown_text)
}
