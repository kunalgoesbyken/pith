use std::cell::RefCell;

pub struct Slot<T>(RefCell<Option<T>>);

impl<T> Slot<T> {
    pub const fn new() -> Self {
        Self(RefCell::new(None))
    }

    pub fn install(&self, value: T) {
        *self.0.borrow_mut() = Some(value);
    }

    pub fn clear(&self) {
        *self.0.borrow_mut() = None;
    }

    pub fn read<R>(&self, view: impl FnOnce(&T) -> R) -> Option<R> {
        self.0.borrow().as_ref().map(view)
    }

    pub fn update<R>(&self, change: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.0.borrow_mut().as_mut().map(change)
    }
}
