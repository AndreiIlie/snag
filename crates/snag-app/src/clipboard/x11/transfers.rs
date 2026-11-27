use crate::clipboard::TRANSFER_TIMEOUT;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    time::Instant,
};
use x11rb::protocol::xproto::{Atom, Window};

pub(super) type Property = (Window, Atom);

struct Transfer {
    offset: usize,
    deadline: Instant,
}

pub(super) struct Transfers {
    pub owning: bool,
    pub limit: usize,
    length: usize,
    active: HashMap<Property, Transfer>,
}

impl Transfers {
    pub fn new(length: usize, limit: usize) -> Self {
        Self {
            owning: true,
            limit,
            length,
            active: HashMap::new(),
        }
    }

    pub fn incremental(&self) -> bool {
        self.length > self.limit
    }
    pub fn active(&self, key: Property) -> bool {
        self.active.contains_key(&key)
    }
    pub fn serving(&self) -> bool {
        self.owning || !self.active.is_empty()
    }
    pub fn watching(&self, window: Window) -> bool {
        self.active.keys().any(|key| key.0 == window)
    }

    pub fn start(&mut self, key: Property, now: Instant) {
        self.active.insert(
            key,
            Transfer {
                offset: 0,
                deadline: now + TRANSFER_TIMEOUT,
            },
        );
    }
    pub fn earliest_deadline(&self) -> Option<Instant> {
        self.active.values().map(|transfer| transfer.deadline).min()
    }
    pub fn expire(&mut self, now: Instant) -> HashSet<Window> {
        self.active
            .extract_if(|_, transfer| transfer.deadline <= now)
            .map(|((window, _), _)| window)
            .collect()
    }
    pub fn forget_window(&mut self, window: Window) {
        self.active.retain(|key, _| key.0 != window);
    }
    pub fn cancel(&mut self, key: Property) {
        self.active.remove(&key);
    }

    // Each deletion acknowledges the previous property. The last reply is empty.
    pub fn chunk(&self, key: Property) -> Option<Range<usize>> {
        let start = self.active.get(&key)?.offset;
        let end = start.saturating_add(self.limit).min(self.length);
        Some(start..end)
    }
    pub fn sent(&mut self, key: Property, chunk: Range<usize>, now: Instant) {
        if chunk.is_empty() {
            self.cancel(key);
        } else if let Some(transfer) = self.active.get_mut(&key) {
            transfer.offset = chunk.end;
            transfer.deadline = now + TRANSFER_TIMEOUT;
        }
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
        let now = Instant::now();
        transfers.start(paste, now);
        transfers.start((2, 3), now);
        assert_eq!(transfers.chunk(paste), Some(0..6));
        transfers.sent(paste, 0..6, now);
        transfers.owning = false;
        transfers.forget_window(2);
        assert!(!transfers.active((2, 3)));
        assert!(transfers.serving());
        assert_eq!(transfers.chunk(paste), Some(6..10));
        transfers.sent(paste, 6..10, now);
        assert!(transfers.serving());
        assert_eq!(transfers.chunk(paste), Some(10..10));
        transfers.sent(paste, 10..10, now);
        assert!(!transfers.serving());
    }

    #[test]
    fn expires_only_idle_transfers_and_releases_the_last_paste_after_loss() {
        let mut transfers = Transfers::new(10, 6);
        let start = Instant::now();
        let progress = start + TRANSFER_TIMEOUT / 2;
        let first_deadline = start + TRANSFER_TIMEOUT;
        let later_deadline = progress + TRANSFER_TIMEOUT;
        let idle = (1, 2);
        let reading = (1, 3);
        transfers.start(idle, start);
        transfers.start(reading, start);
        transfers.sent(reading, 0..6, progress);
        assert_eq!(transfers.earliest_deadline(), Some(first_deadline));
        assert!(transfers.expire(progress).is_empty());
        assert_eq!(transfers.expire(first_deadline), HashSet::from([1]));
        assert!(!transfers.active(idle));
        assert!(transfers.watching(1));
        assert_eq!(transfers.chunk(reading), Some(6..10));
        assert_eq!(transfers.earliest_deadline(), Some(later_deadline));

        transfers.start(idle, first_deadline);
        assert_eq!(transfers.chunk(idle), Some(0..6));
        transfers.owning = false;
        transfers.expire(later_deadline);
        assert!(transfers.serving());
        transfers.expire(first_deadline + TRANSFER_TIMEOUT);
        assert!(!transfers.serving());
        assert_eq!(transfers.earliest_deadline(), None);
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
