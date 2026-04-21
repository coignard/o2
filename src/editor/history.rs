// This file is part of o2.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Undo/redo history for the grid.
//!
//! Stores a bounded ring of cell snapshots. The maximum depth is 30 entries.
//! New entries are deduplicated against the most recent snapshot so that
//! repeated no-op operations do not consume history slots.

use std::collections::VecDeque;

/// A bounded snapshot stack supporting undo and redo.
///
/// Snapshots are pushed via [`record`](History::record) and are deduplicated:
/// if the new state is identical to the most recent entry, no snapshot is
/// stored. This prevents spurious undo steps when nothing has actually changed.
#[derive(Debug)]
pub struct History {
    /// The snapshot ring, oldest entry at index 0.
    pub frames: VecDeque<Vec<char>>,
    /// Index of the current position within the ring (i.e. the most recently
    /// applied snapshot). Undo decrements this; redo increments it.
    pub index: usize,
    /// Maximum number of snapshots to retain before discarding the oldest.
    pub limit: usize,
}

impl History {
    /// Creates an empty history with no snapshots.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::editor::history::History;
    ///
    /// let hist = History::new();
    /// assert_eq!(hist.frames.len(), 0);
    /// assert_eq!(hist.index, 0);
    /// ```
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(32),
            index: 0,
            limit: 100,
        }
    }

    /// Records a new snapshot, discarding any redo history beyond the current
    /// position.
    ///
    /// If `cells` is identical to the most recent snapshot it is silently
    /// ignored. The ring is trimmed to [`limit`](History::limit) entries by
    /// dropping the oldest snapshot when the limit is exceeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::editor::history::History;
    ///
    /// let mut hist = History::new();
    /// hist.record(&['a', 'b']);
    /// hist.record(&['a', 'b']); // duplicate: ignored
    /// assert_eq!(hist.frames.len(), 1);
    /// hist.record(&['c', 'd']);
    /// assert_eq!(hist.frames.len(), 2);
    /// ```
    pub fn record(&mut self, cells: &[char]) {
        if self.limit == 0 {
            return;
        }

        if !self.frames.is_empty()
            && self.index < self.frames.len()
            && self.frames[self.index] == cells
        {
            return;
        }

        if self.index + 1 < self.frames.len() {
            self.frames.truncate(self.index + 1);
        }
        self.frames.push_back(cells.to_vec());
        if self.frames.len() > self.limit {
            self.frames.pop_front();
        }
        self.index = self.frames.len().saturating_sub(1);
    }

    /// Reverts `cells` to the previous snapshot.
    ///
    /// Does nothing if already at the beginning of the history.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::editor::history::History;
    ///
    /// let mut hist = History::new();
    /// hist.record(&['1']);
    /// hist.record(&['2']);
    /// let mut cells = vec!['2'];
    /// hist.undo(&mut cells);
    /// assert_eq!(cells, vec!['1']);
    /// ```
    pub fn undo(&mut self, cells: &mut Vec<char>) {
        if self.index > 0 {
            self.index -= 1;
            *cells = self.frames[self.index].clone();
        }
    }

    /// Re-applies the next snapshot in the redo stack.
    ///
    /// Does nothing if already at the most recent snapshot.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::editor::history::History;
    ///
    /// let mut hist = History::new();
    /// hist.record(&['1']);
    /// hist.record(&['2']);
    /// let mut cells = vec!['2'];
    /// hist.undo(&mut cells);
    /// hist.redo(&mut cells);
    /// assert_eq!(cells, vec!['2']);
    /// ```
    pub fn redo(&mut self, cells: &mut Vec<char>) {
        if self.index + 1 < self.frames.len() {
            self.index += 1;
            *cells = self.frames[self.index].clone();
        }
    }

    /// Discards all snapshots and resets the index to zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::editor::history::History;
    ///
    /// let mut hist = History::new();
    /// hist.record(&['x']);
    /// hist.clear();
    /// assert_eq!(hist.frames.len(), 0);
    /// assert_eq!(hist.index, 0);
    /// ```
    pub fn clear(&mut self) {
        self.frames.clear();
        self.index = 0;
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_record_dedup() {
        let mut hist = History::new();
        let state1 = vec!['a'];
        let state2 = vec!['b'];

        hist.record(&state1);
        assert_eq!(hist.frames.len(), 1);

        hist.record(&state1);
        assert_eq!(hist.frames.len(), 1);

        hist.record(&state2);
        assert_eq!(hist.frames.len(), 2);
    }

    #[test]
    fn test_history_limit() {
        let mut hist = History::new();
        hist.limit = 30;
        for i in 0..40 {
            hist.record(&vec![std::char::from_u32(i + 65).unwrap()]);
        }
        assert_eq!(hist.frames.len(), 30);
        assert_eq!(hist.index, 29);
        assert_eq!(hist.frames[0], vec![std::char::from_u32(10 + 65).unwrap()]);
        assert_eq!(hist.frames[29], vec![std::char::from_u32(39 + 65).unwrap()]);
    }

    #[test]
    fn test_history_undo_redo() {
        let mut hist = History::new();
        let mut current = vec!['0'];

        hist.record(&vec!['1']);
        hist.record(&vec!['2']);
        hist.record(&vec!['3']);

        assert_eq!(hist.index, 2);

        hist.undo(&mut current);
        assert_eq!(current, vec!['2']);
        assert_eq!(hist.index, 1);

        hist.undo(&mut current);
        assert_eq!(current, vec!['1']);
        assert_eq!(hist.index, 0);

        hist.undo(&mut current);
        assert_eq!(current, vec!['1']);

        hist.redo(&mut current);
        assert_eq!(current, vec!['2']);
        assert_eq!(hist.index, 1);

        hist.redo(&mut current);
        assert_eq!(current, vec!['3']);
        assert_eq!(hist.index, 2);

        hist.redo(&mut current);
        assert_eq!(current, vec!['3']);
    }

    #[test]
    fn test_history_fork() {
        let mut hist = History::new();
        let mut current = vec!['0'];

        hist.record(&vec!['1']);
        hist.record(&vec!['2']);
        hist.record(&vec!['3']);

        hist.undo(&mut current);
        hist.undo(&mut current);

        hist.record(&vec!['A']);
        assert_eq!(hist.frames.len(), 2);
        assert_eq!(hist.frames[0], vec!['1']);
        assert_eq!(hist.frames[1], vec!['A']);
        assert_eq!(hist.index, 1);
    }

    #[test]
    fn test_history_clear() {
        let mut hist = History::new();
        hist.record(&vec!['C']);
        hist.record(&vec!['h']);
        hist.record(&vec!['a']);
        hist.record(&vec!['r']);
        hist.record(&vec!['l']);
        hist.record(&vec!['o']);
        hist.record(&vec!['t']);
        hist.record(&vec!['t']);
        hist.record(&vec!['e']);
        hist.clear();
        assert_eq!(hist.frames.len(), 0);
        assert_eq!(hist.index, 0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_history_never_exceeds_limit(
            limit in 0usize..50,
            pushes in 0usize..100
        ) {
            let mut hist = History::new();
            hist.limit = limit;

            for i in 0..pushes {
                hist.record(&[std::char::from_digit((i % 36) as u32, 36).unwrap_or('0')]);
            }

            assert!(hist.frames.len() <= limit);
            if limit > 0 && pushes > 0 {
                assert!(hist.index < limit);
            } else {
                assert_eq!(hist.index, 0);
            }
        }
    }
}
