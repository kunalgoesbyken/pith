use std::cell::RefCell;

pub struct Slot<T>(RefCell<Option<T>>);

impl<T: Clone> Slot<T> {
    pub const fn new() -> Self {
        Self(RefCell::new(None))
    }

    pub fn install(&self, value: T) {
        *self.0.borrow_mut() = Some(value);
    }

    pub fn clear(&self) {
        *self.0.borrow_mut() = None;
    }

    pub fn current(&self) -> Option<T> {
        self.0.borrow().clone()
    }

    pub fn update<R>(&self, change: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.0.borrow_mut().as_mut().map(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_holds_one_value_until_cleared() {
        let slot = Slot::new();
        assert_eq!(slot.current(), None::<i32>);
        assert_eq!(slot.update(|value| *value += 1), None);
        slot.install(1);
        assert_eq!(slot.update(|value| *value += 1), Some(()));
        assert_eq!(slot.current(), Some(2));
        slot.clear();
        assert_eq!(slot.current(), None);
    }
}
