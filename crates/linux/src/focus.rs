use atspi::events::{Event, ObjectEvents};
use atspi::proxy::accessible::ObjectRefExt;
use atspi::{ObjectRefOwned, State};
use zbus::Connection;

#[derive(Default)]
pub(crate) struct Tracker {
    focused: Option<ObjectRefOwned>,
}

impl Tracker {
    pub(crate) fn focused(&self) -> Option<&ObjectRefOwned> {
        self.focused.as_ref()
    }

    fn is_focused(&self, item: &ObjectRefOwned) -> bool {
        self.focused.as_ref() == Some(item)
    }

    pub(crate) async fn apply(&mut self, connection: &Connection, event: Event) -> bool {
        let Event::Object(event) = event else {
            return false;
        };
        match event {
            ObjectEvents::StateChanged(change) if change.state == State::Focused => {
                if change.enabled {
                    self.focused = Some(change.item);
                    true
                } else if self.is_focused(&change.item) {
                    self.focused = None;
                    true
                } else {
                    false
                }
            }
            ObjectEvents::TextCaretMoved(moved) => {
                if self.is_focused(&moved.item) {
                    true
                } else if holds_focus(connection, &moved.item).await {
                    self.focused = Some(moved.item);
                    true
                } else {
                    false
                }
            }
            ObjectEvents::TextChanged(changed) => self.is_focused(&changed.item),
            ObjectEvents::TextSelectionChanged(selection) => self.is_focused(&selection.item),
            _ => false,
        }
    }
}

async fn holds_focus(connection: &Connection, item: &ObjectRefOwned) -> bool {
    let Ok(node) = item.as_accessible_proxy(connection).await else {
        return false;
    };
    node.get_state()
        .await
        .is_ok_and(|state| state.contains(State::Focused))
}
