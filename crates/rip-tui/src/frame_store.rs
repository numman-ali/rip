use std::collections::VecDeque;

use rip_kernel::Event;

#[derive(Debug, Clone)]
pub struct FrameStore {
    base_seq: u64,
    frames: VecDeque<Event>,
    max_frames: usize,
}

impl FrameStore {
    pub fn new(max_frames: usize) -> Self {
        Self {
            base_seq: 0,
            frames: VecDeque::new(),
            max_frames: max_frames.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn first_seq(&self) -> Option<u64> {
        self.frames.front().map(|event| event.seq)
    }

    pub fn last_seq(&self) -> Option<u64> {
        self.frames.back().map(|event| event.seq)
    }

    pub fn push(&mut self, event: Event) {
        if self.frames.is_empty() {
            self.base_seq = event.seq;
        }

        if self.frames.len() >= self.max_frames {
            self.frames.pop_front();
            self.base_seq = self.base_seq.saturating_add(1);
        }

        self.frames.push_back(event);
    }

    pub fn get_by_seq(&self, seq: u64) -> Option<&Event> {
        let idx = self.index_of_seq(seq)?;
        self.frames.get(idx)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.frames.iter()
    }

    pub fn index_of_seq(&self, seq: u64) -> Option<usize> {
        let len = self.frames.len();
        if len == 0 {
            return None;
        }
        if seq < self.base_seq {
            return None;
        }
        let idx = usize::try_from(seq - self.base_seq).ok()?;
        if idx >= len {
            return None;
        }
        Some(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{Event, EventKind};

    fn event(seq: u64) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        }
    }

    #[test]
    fn caps_frames_and_evictions_shift_seq_window() {
        let mut store = FrameStore::new(2);
        store.push(event(10));
        store.push(event(11));
        assert_eq!(store.first_seq(), Some(10));
        assert_eq!(store.last_seq(), Some(11));
        assert!(store.get_by_seq(10).is_some());

        store.push(event(12));
        assert_eq!(store.len(), 2);
        assert_eq!(store.first_seq(), Some(11));
        assert_eq!(store.last_seq(), Some(12));
        assert!(store.get_by_seq(10).is_none());
        assert!(store.get_by_seq(11).is_some());
        assert!(store.get_by_seq(12).is_some());
    }
}
