use std::time::Instant;

use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{ObjectRefOwned, RelationType, Role, State};
use pith_core::{CueKind, NearbyCache, RawCue, WALK, log};
use zbus::Connection;

use crate::bounded::within;

const WINDOW_ROLES: [Role; 4] = [Role::Frame, Role::Window, Role::Dialog, Role::Application];

pub(crate) async fn cues(
    connection: &Connection,
    cache: &mut NearbyCache,
    field: &ObjectRefOwned,
    key: (u64, String),
    before_empty: bool,
) -> Vec<RawCue> {
    if cache.stale(&key, before_empty) {
        cache.fill(key, read(connection, field).await);
    }
    cache.cues().to_vec()
}

async fn read(connection: &Connection, field: &ObjectRefOwned) -> Vec<RawCue> {
    let started = Instant::now();
    let mut walk = Walk {
        connection,
        started,
        cues: Vec::new(),
        thread_found: false,
    };
    let _ = within(WALK.deadline * 2, walk.run(field)).await;
    log(&format!(
        "nearby: {} cues, {} ms",
        walk.cues.len(),
        started.elapsed().as_millis()
    ));
    walk.cues
}

struct Walk<'a> {
    connection: &'a Connection,
    started: Instant,
    cues: Vec<RawCue>,
    thread_found: bool,
}

impl Walk<'_> {
    fn spent(&self) -> bool {
        self.started.elapsed() >= WALK.deadline
    }

    async fn run(&mut self, field: &ObjectRefOwned) {
        if let Ok(node) = field.as_accessible_proxy(self.connection).await {
            self.labels(&node).await;
        }
        let mut current = field.clone();
        for _ in 0..WALK.levels {
            if self.spent() {
                return;
            }
            let Some(parent) = self.parent_of(&current).await else {
                return;
            };
            self.siblings(&parent, &current).await;
            current = parent;
        }
    }

    async fn parent_of(&self, current: &ObjectRefOwned) -> Option<ObjectRefOwned> {
        let node = current.as_accessible_proxy(self.connection).await.ok()?;
        if WINDOW_ROLES.contains(&node.get_role().await.ok()?) {
            return None;
        }
        node.parent().await.ok().filter(|parent| !parent.is_null())
    }

    async fn labels(&mut self, node: &AccessibleProxy<'_>) {
        let labelled_by = self.labelled_by(node).await;
        let own = [node.name().await, node.description().await];
        for text in labelled_by.into_iter().chain(own.into_iter().flatten()) {
            self.cues
                .push(RawCue::new(CueKind::FieldLabel, None, &text));
        }
    }

    async fn labelled_by(&self, node: &AccessibleProxy<'_>) -> Option<String> {
        let relations = node.get_relation_set().await.ok()?;
        let (_, labels) = relations
            .into_iter()
            .find(|(kind, _)| *kind == RelationType::LabelledBy)?;
        let label = labels.first()?;
        label
            .as_accessible_proxy(self.connection)
            .await
            .ok()?
            .name()
            .await
            .ok()
    }

    async fn siblings(&mut self, parent: &ObjectRefOwned, current: &ObjectRefOwned) {
        let Ok(node) = parent.as_accessible_proxy(self.connection).await else {
            return;
        };
        let Ok(children) = node.get_children().await else {
            return;
        };
        let Some(position) = children.iter().position(|child| child == current) else {
            return;
        };
        let nearest = children[..position].iter().rev().take(WALK.siblings);
        for sibling in nearest {
            if self.spent() {
                return;
            }
            self.sibling(sibling).await;
        }
    }

    async fn sibling(&mut self, sibling: &ObjectRefOwned) {
        let Ok(node) = sibling.as_accessible_proxy(self.connection).await else {
            return;
        };
        let (Ok(role), Ok(state)) = (node.get_role().await, node.get_state().await) else {
            return;
        };
        if !state.contains(State::Showing) {
            return;
        }
        match role {
            Role::Label | Role::Heading | Role::Static | Role::Caption => {
                if let Ok(name) = node.name().await {
                    self.cues.push(RawCue::new(CueKind::Text, None, &name));
                }
            }
            Role::Entry => self.form_value(&node).await,
            Role::List
            | Role::Table
            | Role::TreeTable
            | Role::DocumentFrame
            | Role::DocumentWeb
                if !self.thread_found =>
            {
                self.thread_found = true;
                self.latest_message(sibling).await;
            }
            _ => {}
        }
    }

    async fn form_value(&mut self, node: &AccessibleProxy<'_>) {
        let (Ok(name), Some(value)) = (node.name().await, self.text_of(node).await) else {
            return;
        };
        self.cues
            .push(RawCue::new(CueKind::FormValue, Some(&name), &value));
    }

    async fn text_of(&self, node: &AccessibleProxy<'_>) -> Option<String> {
        let text = node.proxies().await.ok()?.text().await.ok()?;
        let count = text.character_count().await.ok()?;
        let limit = i32::try_from(WALK.raw_chars).unwrap_or(i32::MAX);
        text.get_text(count.saturating_sub(limit).max(0), count)
            .await
            .ok()
    }

    async fn latest_message(&mut self, thread: &ObjectRefOwned) {
        let mut latest = thread.clone();
        let mut message = None;
        for _ in 0..WALK.descent {
            if self.spent() {
                break;
            }
            let Some((child, text)) = self.last_child(&latest).await else {
                break;
            };
            message = Some(text)
                .filter(|text| !text.trim().is_empty())
                .or(message);
            latest = child;
        }
        if let Some(text) = message {
            self.cues.push(RawCue::new(CueKind::Message, None, &text));
        }
    }

    async fn last_child(&self, parent: &ObjectRefOwned) -> Option<(ObjectRefOwned, String)> {
        let node = parent.as_accessible_proxy(self.connection).await.ok()?;
        let count = node.child_count().await.ok()?;
        let child = node.get_child_at_index(count.checked_sub(1)?).await.ok()?;
        let proxy = child.as_accessible_proxy(self.connection).await.ok()?;
        let name = proxy.name().await.unwrap_or_default();
        let text = if name.trim().is_empty() {
            self.text_of(&proxy).await.unwrap_or_default()
        } else {
            name
        };
        Some((child, text))
    }
}
