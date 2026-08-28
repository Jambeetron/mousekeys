// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Virtual keyboard used to re-inject keys we do not consume.
//!
//! While mouse-keys is enabled, every physical keyboard is grabbed via
//! `EVIOCGRAB`, so the kernel stops delivering key events to the rest of
//! the system. To keep normal typing and key bindings working, any key we
//! do not interpret (anything that is not on the numpad and not the
//! Super+H toggle chord) is re-emitted on this virtual keyboard.

use std::io;

use evdev::{AttributeSet, EventType, InputEvent, KeyCode, uinput::VirtualDevice};

const DEVICE_NAME: &str = "mousekeys virtual keyboard";

/// Wraps a uinput device that masquerades as a regular keyboard.
pub struct Keyboard {
    dev: VirtualDevice,
}

impl Keyboard {
    /// Create the virtual keyboard advertising the given key codes. Every
    /// key we may need to forward must be present in `keys`, otherwise the
    /// kernel will reject the write.
    pub fn new(keys: AttributeSet<KeyCode>) -> io::Result<Self> {
        let dev = VirtualDevice::builder()?
            .name(DEVICE_NAME)
            .with_keys(&keys)?
            .build()?;
        Ok(Self { dev })
    }

    /// Re-emit a single raw `EV_KEY` event on the virtual keyboard.
    /// Non-`EV_KEY` events are silently dropped: LEDs, MSC_SCAN, etc. are
    /// not needed for normal typing and forwarding them could confuse
    /// libinput's state machine.
    pub fn forward(&mut self, ev: &InputEvent) -> io::Result<()> {
        if ev.event_type() != EventType::KEY {
            return Ok(());
        }
        self.dev.emit(std::slice::from_ref(ev))
    }
}
