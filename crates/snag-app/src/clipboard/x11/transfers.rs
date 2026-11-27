use std::{collections::HashMap, ops::Range};
use x11rb::protocol::xproto::{Atom, Window};

pub(super) type Property = (Window, Atom);

pub(super) struct Transfers {
    pub owning: bool,
    pub limit: usize,
    length: usize,
    offsets: HashMap<Property, usize>,
}

impl Transfers {
    pub fn new(length: usize, limit: usize) -> Self {
        Self {
            owning: true,
            limit,
            length,
            offsets: HashMap::new(),
        }
    }

    pub fn incremental(&self) -> bool {
        self.length > self.limit
    }
    pub fn active(&self, key: Property) -> bool {
        self.offsets.contains_key(&key)
    }
    pub fn serving(&self) -> bool {
        self.owning || !self.offsets.is_empty()
    }
    pub fn watching(&self, window: Window) -> bool {
        self.offsets.keys().any(|key| key.0 == window)
    }

    pub fn start(&mut self, key: Property) {
        self.offsets.insert(key, 0);
    }
    pub fn forget_window(&mut self, window: Window) {
        self.offsets.retain(|key, _| key.0 != window);
    }
    pub fn cancel(&mut self, key: Property) {
        self.offsets.remove(&key);
    }

    // Each deletion acknowledges the previous property. The last reply is empty.
    pub fn next(&mut self, key: Property) -> Option<Range<usize>> {
        let offset = self.offsets.get_mut(&key)?;
        let start = *offset;
        let end = start.saturating_add(self.limit).min(self.length);
        *offset = end;
        if start == end {
            self.cancel(key);
        }
        Some(start..end)
    }
}

pub(super) fn convert_pairs(
    pairs: &mut [u32],
    list_property: Atom,
    mut convert: impl FnMut(Atom, Atom) -> Result<bool, x11rb::errors::ReplyError>,
) -> Result<(), x11rb::errors::ReplyError> {
    for [target, property] in pairs.as_chunks_mut::<2>().0 {
        if *property == 0 || *property == list_property || !convert(*target, *property)? {
            *property = 0;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_incr_only_above_the_payload_limit() {
        for (bytes, expected) in [(7, false), (8, false), (9, true)] {
            assert_eq!(Transfers::new(bytes, 8).incremental(), expected);
        }
    }

    #[test]
    fn drains_an_active_paste_after_replacement_and_releases_destroyed_windows() {
        let mut transfers = Transfers::new(10, 6);
        let paste = (1, 2);
        transfers.start(paste);
        transfers.start((2, 3));
        assert_eq!(transfers.next(paste), Some(0..6));
        transfers.owning = false;
        transfers.forget_window(2);
        assert!(!transfers.active((2, 3)));
        assert!(transfers.serving());
        assert_eq!(transfers.next(paste), Some(6..10));
        assert!(transfers.serving());
        assert_eq!(transfers.next(paste), Some(10..10));
        assert!(!transfers.serving());
    }

    #[test]
    fn multiple_converts_each_pair_and_marks_failed_properties() {
        let mut pairs = [1, 10, 2, 20, 1, 30, 1, 99];
        let mut called = Vec::new();
        convert_pairs(&mut pairs, 99, |target, property| {
            called.push((target, property));
            Ok(target == 1)
        })
        .unwrap();
        assert_eq!(called, [(1, 10), (2, 20), (1, 30)]);
        assert_eq!(pairs, [1, 10, 2, 0, 1, 30, 1, 0]);
    }
}
