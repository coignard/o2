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

//! BPM clock management.
//!
//! Provides methods on [`EditorState`] for manipulating the tempo. The clock supports
//! two modes of operation:
//!
//! - **Immediate** (`set_bpm`, `mod_bpm`): both `bpm` and `bpm_target` are
//!   updated at once, producing an instantaneous change.
//! - **Animated** (`set_bpm_target`, `mod_bpm_target`): only `bpm_target` is
//!   updated; `bpm` converges towards it one step per frame inside
//!   [`EditorState::operate`].
//!
//! All tempo values are clamped to the range `[1, 360]` BPM.
//!
//! # Clock model
//!
//! The Orca engine runs at four ticks per beat (quarter note).  MIDI Beat
//! Clock (24 PPQN) is sent at six sub-ticks per engine tick, giving 24 clock
//! pulses per beat as required by the MIDI specification.  Both streams share
//! the same phase-locked counter so they remain aligned regardless of system
//! scheduling latency.

use crate::core::app::EditorState;

impl EditorState {
    /// Sets [`bpm_target`](EditorState::bpm_target) to `target`, clamped to `[1, 360]`.
    ///
    /// The current BPM will animate towards this value, changing by one step
    /// per frame in [`EditorState::operate`].
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// app.set_bpm_target(150);
    /// assert_eq!(app.bpm_target, 150);
    /// app.set_bpm_target(999);
    /// assert_eq!(app.bpm_target, 360);
    /// ```
    pub fn set_bpm_target(&mut self, target: usize) {
        self.bpm_target = target.clamp(1, 360);
    }

    /// Sets both [`bpm`](EditorState::bpm) and [`bpm_target`](EditorState::bpm_target) to
    /// `bpm` immediately, with no animation.
    ///
    /// The value is clamped to `[1, 360]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// app.set_bpm(140);
    /// assert_eq!(app.bpm, 140);
    /// assert_eq!(app.bpm_target, 140);
    /// ```
    pub fn set_bpm(&mut self, bpm: usize) {
        let c = bpm.clamp(1, 360);
        self.bpm = c;
        self.bpm_target = c;
    }

    /// Adjusts [`bpm_target`](EditorState::bpm_target) by `diff` BPM, clamped to
    /// `[1, 360]`.
    ///
    /// The change is animated: [`bpm`](EditorState::bpm) will converge gradually over
    /// subsequent frames.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// app.set_bpm_target(120);
    /// app.mod_bpm_target(10);
    /// assert_eq!(app.bpm_target, 130);
    /// ```
    pub fn mod_bpm_target(&mut self, diff: isize) {
        let new_target = (self.bpm_target as isize + diff).clamp(1, 360) as usize;
        self.bpm_target = new_target;
    }

    /// Adjusts both [`bpm`](EditorState::bpm) and [`bpm_target`](EditorState::bpm_target) by
    /// `diff` BPM immediately, with no animation.
    ///
    /// The result is clamped to `[1, 360]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use o2_rs::core::app::EditorState;
    ///
    /// let mut app = EditorState::new(10, 10, 1, 100);
    /// app.set_bpm(120);
    /// app.mod_bpm(30);
    /// assert_eq!(app.bpm, 150);
    /// assert_eq!(app.bpm_target, 150);
    /// ```
    pub fn mod_bpm(&mut self, diff: isize) {
        let new_val = (self.bpm as isize + diff).clamp(1, 360) as usize;
        self.bpm = new_val;
        self.bpm_target = new_val;
    }
}

#[cfg(test)]
mod tests {
    use crate::core::app::EditorState;

    #[test]
    fn test_set_bpm_clamps() {
        let mut app = EditorState::new(10, 10, 1, 100);
        app.set_bpm(120);
        assert_eq!(app.bpm, 120);
        assert_eq!(app.bpm_target, 120);

        app.set_bpm(0);
        assert_eq!(app.bpm, 1);

        app.set_bpm(400);
        assert_eq!(app.bpm, 360);
    }

    #[test]
    fn test_set_bpm_target_clamps() {
        let mut app = EditorState::new(10, 10, 1, 100);
        app.set_bpm_target(150);
        assert_eq!(app.bpm_target, 150);

        app.set_bpm_target(0);
        assert_eq!(app.bpm_target, 1);

        app.set_bpm_target(999);
        assert_eq!(app.bpm_target, 360);
    }

    #[test]
    fn test_mod_bpm() {
        let mut app = EditorState::new(10, 10, 1, 100);
        app.set_bpm(120);

        app.mod_bpm(69);
        assert_eq!(app.bpm, 189);
        assert_eq!(app.bpm_target, 189);

        app.mod_bpm(-189);
        assert_eq!(app.bpm, 1);

        app.mod_bpm(360);
        assert_eq!(app.bpm, 360);
    }

    #[test]
    fn test_mod_bpm_target() {
        let mut app = EditorState::new(10, 10, 1, 100);
        app.set_bpm_target(120);

        app.mod_bpm_target(50);
        assert_eq!(app.bpm_target, 170);

        app.mod_bpm_target(-200);
        assert_eq!(app.bpm_target, 1);

        app.mod_bpm_target(400);
        assert_eq!(app.bpm_target, 360);
    }
}

#[cfg(test)]
mod property_tests {
    use crate::core::app::EditorState;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_bpm_always_clamped(
            initial in any::<usize>(),
            target in any::<usize>(),
            mod_val in any::<isize>(),
            mod_target in any::<isize>()
        ) {
            let mut app = EditorState::new(10, 10, 1, 100);

            app.set_bpm(initial);
            assert!(app.bpm >= 1 && app.bpm <= 360);
            assert_eq!(app.bpm, app.bpm_target);

            app.set_bpm_target(target);
            assert!(app.bpm_target >= 1 && app.bpm_target <= 360);

            app.mod_bpm(mod_val);
            assert!(app.bpm >= 1 && app.bpm <= 360);
            assert_eq!(app.bpm, app.bpm_target);

            app.mod_bpm_target(mod_target);
            assert!(app.bpm_target >= 1 && app.bpm_target <= 360);
        }
    }
}
