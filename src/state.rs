// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Shared runtime state: enabled flag, mode, speeds, held button.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::keys::Mode;

// Motion speed bounds (pixels per movement event).
const SPEED_MIN: u32 = 4;
const SPEED_MAX: u32 = 200;
const SPEED_STEP: u32 = 4;
const SPEED_DEFAULT: u32 = 20;

// Scroll speed bounds (wheel ticks per scroll event).
const SCROLL_MIN: u32 = 1;
const SCROLL_MAX: u32 = 20;
const SCROLL_STEP: u32 = 1;
const SCROLL_DEFAULT: u32 = 1;

/// Central mutable state shared between the event loop and any helpers.
///
/// All fields are atomic, so access requires no lock and no `unsafe`.
pub struct State {
    /// Whether mouse-keys is intercepting the numpad.
    enabled: AtomicBool,
    /// Current motion mode.
    mode: AtomicBool,
    /// Current motion speed (pixels per movement event).
    speed: AtomicU32,
    /// Current scroll speed (wheel ticks per scroll event).
    scroll_speed: AtomicU32,
    /// Whether the left button is currently held down (by us).
    left_held: AtomicBool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            mode: AtomicBool::new(false),
            speed: AtomicU32::new(SPEED_DEFAULT),
            scroll_speed: AtomicU32::new(SCROLL_DEFAULT),
            left_held: AtomicBool::new(false),
        }
    }
}

impl State {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&self, value: bool) {
        self.enabled.store(value, Ordering::Release);
    }

    pub fn mode(&self) -> Mode {
        if self.mode.load(Ordering::Acquire) {
            Mode::Scroll
        } else {
            Mode::Movement
        }
    }

    pub fn toggle_mode(&self) -> Mode {
        let was_scroll = self.mode.fetch_xor(true, Ordering::AcqRel);
        if was_scroll {
            Mode::Movement
        } else {
            Mode::Scroll
        }
    }

    pub fn speed(&self) -> i32 {
        self.speed.load(Ordering::Acquire) as i32
    }

    /// Set the motion speed, clamping into the legal range. Returns the
    /// resulting value.
    pub fn set_speed(&self, value: u32) -> u32 {
        let clamped = value.clamp(SPEED_MIN, SPEED_MAX);
        self.speed.store(clamped, Ordering::Release);
        clamped
    }

    /// Increase motion speed by one step, clamped to SPEED_MAX.
    pub fn speed_up(&self) -> u32 {
        let _ = self
            .speed
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_add(SPEED_STEP).min(SPEED_MAX))
            });
        self.speed.load(Ordering::Acquire)
    }

    /// Decrease motion speed by one step, clamped to SPEED_MIN.
    pub fn speed_down(&self) -> u32 {
        let _ = self
            .speed
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_sub(SPEED_STEP).max(SPEED_MIN))
            });
        self.speed.load(Ordering::Acquire)
    }

    pub fn scroll_speed(&self) -> i32 {
        self.scroll_speed.load(Ordering::Acquire) as i32
    }

    /// Increase scroll speed by one step, clamped to SCROLL_MAX.
    pub fn scroll_speed_up(&self) -> u32 {
        let _ = self
            .scroll_speed
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_add(SCROLL_STEP).min(SCROLL_MAX))
            });
        self.scroll_speed.load(Ordering::Acquire)
    }

    /// Decrease scroll speed by one step, clamped to SCROLL_MIN.
    pub fn scroll_speed_down(&self) -> u32 {
        let _ = self
            .scroll_speed
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_sub(SCROLL_STEP).max(SCROLL_MIN))
            });
        self.scroll_speed.load(Ordering::Acquire)
    }

    pub fn left_held(&self) -> bool {
        self.left_held.load(Ordering::Acquire)
    }

    pub fn set_left_held(&self, held: bool) {
        self.left_held.store(held, Ordering::Release);
    }
}

pub const SPEED_BOUNDS: (u32, u32) = (SPEED_MIN, SPEED_MAX);
pub const SPEED_INITIAL: u32 = SPEED_DEFAULT;
