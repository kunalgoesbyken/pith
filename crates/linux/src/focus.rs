use async_channel::Receiver;
use atspi::events::{Event, ObjectEvents};
use atspi::proxy::accessible::ObjectRefExt;
use atspi::{AtspiError, ObjectRefOwned, State};
use zbus::Connection;

use crate::bounded::{Bounded, bounded};
use crate::identity::Identity;

pub(crate) struct Focus {
    pub(crate) target: ObjectRefOwned,
    identity: Option<Identity>,
}

pub(crate) enum Effect {
    Changed,
    Unchanged,
    Stopped,
}

#[derive(Default)]
pub(crate) struct Tracker {
    focus: Option<Focus>,
    lost: Option<ObjectRefOwned>,
}

impl Focus {
    fn new(target: ObjectRefOwned) -> Self {
        Self {
            target,
            identity: None,
        }
    }

    pub(crate) async fn identity(
        &mut self,
        connection: &Connection,
    ) -> Result<&Identity, AtspiError> {
        let identity = match self.identity.take() {
            Some(known) => known,
            None => Identity::resolve(connection, &self.target).await?,
        };
        Ok(self.identity.insert(identity))
    }
}

impl Tracker {
    pub(crate) fn focus_mut(&mut self) -> Option<&mut Focus> {
        self.focus.as_mut()
    }

    fn is_focused(&self, item: &ObjectRefOwned) -> bool {
        self.focus
            .as_ref()
            .is_some_and(|focus| focus.target == *item)
    }

    pub(crate) fn loss_pending(&self) -> bool {
        self.lost.is_some()
    }

    pub(crate) async fn settle_loss(&mut self, connection: &Connection, held: bool) {
        let Some(lost) = self.lost.take_if(|_| !held) else {
            return;
        };
        if self.is_focused(&lost) && !holds_focus(connection, &lost).await {
            self.focus = None;
        }
    }

    pub(crate) async fn apply(
        &mut self,
        connection: &Connection,
        stopped: &Receiver<()>,
        event: Event,
    ) -> Effect {
        let Event::Object(event) = event else {
            return Effect::Unchanged;
        };
        let changed = match event {
            ObjectEvents::StateChanged(change) if change.state == State::Focused => {
                if change.enabled {
                    self.focus = Some(Focus::new(change.item));
                    self.lost = None;
                    true
                } else {
                    if self.is_focused(&change.item) {
                        self.lost = Some(change.item);
                    }
                    false
                }
            }
            ObjectEvents::TextCaretMoved(moved) => {
                if self.is_focused(&moved.item) {
                    true
                } else if self.focus.is_some() {
                    false
                } else {
                    match bounded(stopped, holds_focus(connection, &moved.item)).await {
                        Bounded::Done(true) => {
                            self.focus = Some(Focus::new(moved.item));
                            true
                        }
                        Bounded::Done(false) | Bounded::TimedOut => false,
                        Bounded::Stopped => return Effect::Stopped,
                    }
                }
            }
            ObjectEvents::TextSelectionChanged(selection) => self.is_focused(&selection.item),
            _ => false,
        };
        if changed {
            Effect::Changed
        } else {
            Effect::Unchanged
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
