struct Entry<T> {
    key: u64,
    used: u64,
    value: T,
}

pub struct Slots<T> {
    entries: Vec<Entry<T>>,
    capacity: usize,
    clock: u64,
}

impl<T> Slots<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity: capacity.max(1),
            clock: 0,
        }
    }

    pub fn acquire(
        &mut self,
        key: u64,
        create: impl FnOnce() -> Result<T, String>,
    ) -> Result<&mut T, String> {
        self.clock += 1;
        let used = self.clock;
        let index = match self.entries.iter().position(|entry| entry.key == key) {
            Some(index) => index,
            None if self.entries.len() < self.capacity => {
                self.entries.push(Entry {
                    key,
                    used,
                    value: create()?,
                });
                self.entries.len() - 1
            }
            None => self.least_recent(),
        };
        let entry = &mut self.entries[index];
        entry.key = key;
        entry.used = used;
        Ok(&mut entry.value)
    }

    fn least_recent(&self) -> usize {
        self.entries
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| entry.used)
            .map_or(0, |(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(created: &mut usize) -> impl FnOnce() -> Result<Vec<u32>, String> + '_ {
        move || {
            *created += 1;
            Ok(Vec::new())
        }
    }

    #[test]
    fn same_key_reuses_its_slot() {
        let mut slots = Slots::new(2);
        let mut created = 0;
        slots.acquire(7, make(&mut created)).unwrap().push(1);
        assert_eq!(slots.acquire(7, make(&mut created)).unwrap(), &vec![1]);
        assert_eq!(created, 1);
    }

    #[test]
    fn fields_keep_separate_slots_up_to_capacity() {
        let mut slots = Slots::new(2);
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap().push(1);
        slots.acquire(2, make(&mut created)).unwrap().push(2);
        assert_eq!(slots.acquire(1, make(&mut created)).unwrap(), &vec![1]);
        assert_eq!(slots.acquire(2, make(&mut created)).unwrap(), &vec![2]);
        assert_eq!(created, 2);
    }

    #[test]
    fn full_pool_recycles_the_least_recently_used_slot() {
        let mut slots = Slots::new(2);
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap().push(1);
        slots.acquire(2, make(&mut created)).unwrap().push(2);
        slots.acquire(1, make(&mut created)).unwrap();
        assert_eq!(slots.acquire(3, make(&mut created)).unwrap(), &vec![2]);
        assert_eq!(slots.acquire(1, make(&mut created)).unwrap(), &vec![1]);
        assert_eq!(created, 2);
    }

    #[test]
    fn recycled_slot_belongs_to_the_new_key() {
        let mut slots = Slots::new(1);
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap().push(1);
        slots.acquire(2, make(&mut created)).unwrap().push(2);
        assert_eq!(slots.acquire(2, make(&mut created)).unwrap(), &vec![1, 2]);
        assert_eq!(created, 1);
    }

    #[test]
    fn failed_creation_leaves_the_pool_unchanged() {
        let mut slots: Slots<Vec<u32>> = Slots::new(2);
        assert!(slots.acquire(1, || Err("boom".into())).is_err());
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap();
        assert_eq!(created, 1);
    }

    #[test]
    fn zero_capacity_still_serves_one_slot() {
        let mut slots = Slots::new(0);
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap();
        slots.acquire(2, make(&mut created)).unwrap();
        assert_eq!(created, 1);
    }
}
