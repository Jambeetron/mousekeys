// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Mapping from evdev keycodes to Mouse Keys actions.
//!
//! IMPORTANT: We only ever consume the dedicated numpad codes (`KEY_KP*`).
//! The "navigation equivalent" codes (`KEY_UP`, `KEY_DOWN`, `KEY_HOME`,
//! etc. that a numpad emits when NumLock is OFF) are NOT consumed, because
//! those same codes are emitted by the main keyboard's arrow/edit keys —
//! evdev gives us no way to tell them apart. Consuming them would break
//! the real arrow keys while mouse-keys is enabled (which is exactly the
//! bug we are fixing).
//!
//! Practical consequence: mouse-keys is active when NumLock is ON (which
//! is the default in Hyprland via `numlock_by_default = true`). With
//! NumLock OFF the numpad behaves as normal navigation — not as Mouse
//! Keys — which is the only correct behaviour given the keycode collision.

use evdev::KeyCode;

/// Current behavioural mode for motion keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Numpad 8/2/4/6/7/9/1/3 move the pointer.
    Movement,
    /// Numpad 8/2 = vertical wheel, 4/6 = horizontal wheel.
    Scroll,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movement => "movement",
            Self::Scroll => "scroll",
        }
    }
}

/// Kinds of mouse actions a mapped key can request.
///
/// `value` is the raw evdev event value: 0 = release, 1 = press,
/// 2 = autorepeat. Motion keys move on press and on every repeat.
/// Hold/drop and click keys only act on the press edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move pointer by `(dx, dy)` scaled by current speed.
    Move { dx: i32, dy: i32 },
    /// Left click: full press+release on the press edge.
    LeftClick,
    /// Middle click on the press edge.
    MiddleClick,
    /// Right click on the press edge.
    RightClick,
    /// Press left button and keep it held.
    HoldLeft,
    /// Release any held button.
    Release,
    /// Increase motion/scroll speed by one step.
    SpeedUp,
    /// Decrease motion/scroll speed by one step.
    SpeedDown,
    /// Switch to the other mode (movement <-> scroll).
    CycleMode,
}

/// Resolve an evdev keycode to an action in the given mode.
///
/// `None` is returned for keys we do not handle (they are re-injected
/// unchanged). Only the dedicated `KEY_KP*` codes are matched here — see
/// the module docs for why the NumLock-off equivalents are excluded.
pub fn resolve(key: KeyCode, mode: Mode) -> Option<Action> {
    use KeyCode as K;
    let action = match key {
        K::KEY_KP8 => Action::Move { dx: 0, dy: -1 },
        K::KEY_KP2 => Action::Move { dx: 0, dy: 1 },
        K::KEY_KP4 => Action::Move { dx: -1, dy: 0 },
        K::KEY_KP6 => Action::Move { dx: 1, dy: 0 },
        K::KEY_KP7 => match mode {
            Mode::Movement => Action::Move { dx: -1, dy: -1 },
            Mode::Scroll => return None,
        },
        K::KEY_KP9 => match mode {
            Mode::Movement => Action::Move { dx: 1, dy: -1 },
            Mode::Scroll => return None,
        },
        K::KEY_KP1 => match mode {
            Mode::Movement => Action::Move { dx: -1, dy: 1 },
            Mode::Scroll => return None,
        },
        K::KEY_KP3 => match mode {
            Mode::Movement => Action::Move { dx: 1, dy: 1 },
            Mode::Scroll => return None,
        },
        K::KEY_KP5 => Action::LeftClick,
        K::KEY_KPSLASH => Action::MiddleClick,
        K::KEY_KPASTERISK => Action::RightClick,
        K::KEY_KPMINUS => Action::SpeedDown,
        K::KEY_KPPLUS => Action::SpeedUp,
        K::KEY_KP0 => Action::HoldLeft,
        K::KEY_KPDOT => Action::Release,
        K::KEY_KPENTER => Action::CycleMode,
        _ => return None,
    };
    Some(action)
}

/// Keys that, while mouse-keys is enabled, we always consume and never
/// re-inject into the virtual keyboard.
///
/// Only the dedicated `KEY_KP*` codes are listed here. The navigation
/// codes (`KEY_UP`, `KEY_HOME`, etc.) are deliberately excluded to keep
/// the real arrow/edit keys of the main keyboard working.
pub fn is_consumed(key: KeyCode) -> bool {
    use KeyCode as K;
    matches!(
        key,
        K::KEY_KP0
            | K::KEY_KP1
            | K::KEY_KP2
            | K::KEY_KP3
            | K::KEY_KP4
            | K::KEY_KP5
            | K::KEY_KP6
            | K::KEY_KP7
            | K::KEY_KP8
            | K::KEY_KP9
            | K::KEY_KPDOT
            | K::KEY_KPPLUS
            | K::KEY_KPMINUS
            | K::KEY_KPASTERISK
            | K::KEY_KPSLASH
            | K::KEY_KPENTER
    )
}
