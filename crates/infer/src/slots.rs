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
    fn slots_are_reused_per_key_and_capacity_is_at_least_one() {
        let rows: [(usize, &[u32], usize, &[u32]); 4] = [
            (2, &[7, 7], 1, &[7, 7]),
            (2, &[1, 2, 1, 2], 2, &[2, 2]),
            (1, &[1, 2, 2], 1, &[1, 2, 2]),
            (0, &[1, 2], 1, &[1, 2]),
        ];
        for row in rows {
            let (capacity, keys, created_expected, contents) = row;
            let mut slots = Slots::new(capacity);
            let mut created = 0;
            let mut last = Vec::new();
            for &key in keys {
                let slot = slots.acquire(u64::from(key), make(&mut created)).unwrap();
                slot.push(key);
                last = slot.clone();
            }
            assert_eq!(created, created_expected, "{row:?}");
            assert_eq!(&last, contents, "{row:?}");
        }
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
    fn failed_creation_leaves_the_pool_unchanged() {
        let mut slots: Slots<Vec<u32>> = Slots::new(2);
        assert!(slots.acquire(1, || Err("boom".into())).is_err());
        let mut created = 0;
        slots.acquire(1, make(&mut created)).unwrap();
        assert_eq!(created, 1);
    }
}
