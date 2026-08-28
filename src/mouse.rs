// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Virtual mouse driven through `/dev/uinput`: motion, clicks, scroll, hold.

use std::io;

use evdev::{
    AttributeSet, EventType, InputEvent, KeyCode, PropType, RelativeAxisCode, uinput::VirtualDevice,
};

const DEVICE_NAME: &str = "mousekeys virtual mouse";

/// Wraps a uinput device that behaves like a 3-button mouse with a wheel.
pub struct Mouse {
    dev: VirtualDevice,
}

impl Mouse {
    /// Create the virtual pointing device.
    pub fn new() -> io::Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::BTN_LEFT);
        keys.insert(KeyCode::BTN_RIGHT);
        keys.insert(KeyCode::BTN_MIDDLE);

        let mut rel = AttributeSet::<RelativeAxisCode>::new();
        rel.insert(RelativeAxisCode::REL_X);
        rel.insert(RelativeAxisCode::REL_Y);
        rel.insert(RelativeAxisCode::REL_WHEEL);
        rel.insert(RelativeAxisCode::REL_HWHEEL);

        // Mark the device as a pointer so libinput routes button events
        // (without this, some compositors ignore BTN_* on a uinput device
        // that has relative axes but no INPUT_PROP_POINTER).
        let mut props = AttributeSet::<PropType>::new();
        props.insert(PropType::POINTER);

        let dev = VirtualDevice::builder()?
            .name(DEVICE_NAME)
            .with_keys(&keys)?
            .with_relative_axes(&rel)?
            .with_properties(&props)?
            .build()?;
        Ok(Self { dev })
    }

    /// Send a button press event. `down` selects press vs release.
    fn button(&mut self, code: KeyCode, down: bool) -> io::Result<()> {
        let value = i32::from(down);
        let ev = raw_event(EventType::KEY, code.0, value);
        self.dev.emit(&[ev])
    }

    /// Emit a left click: press then release as two separate SYN frames so
    /// libinput registers a real click rather than a zero-duration tap.
    pub fn left_click(&mut self) -> io::Result<()> {
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_LEFT.0, 1)])?;
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_LEFT.0, 0)])
    }

    pub fn middle_click(&mut self) -> io::Result<()> {
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_MIDDLE.0, 1)])?;
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_MIDDLE.0, 0)])
    }

    pub fn right_click(&mut self) -> io::Result<()> {
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_RIGHT.0, 1)])?;
        self.dev
            .emit(&[raw_event(EventType::KEY, KeyCode::BTN_RIGHT.0, 0)])
    }

    /// Hold the left button down (no release). Idempotent.
    pub fn hold_left(&mut self) -> io::Result<()> {
        self.button(KeyCode::BTN_LEFT, true)
    }

    /// Release the left button if held (no press). Idempotent.
    pub fn release_left(&mut self) -> io::Result<()> {
        self.button(KeyCode::BTN_LEFT, false)
    }

    /// Relative pointer motion by `(dx, dy)` pixels (each already scaled
    /// by the caller to the current speed). Issues X then Y in one batch.
    pub fn move_rel(&mut self, dx: i32, dy: i32) -> io::Result<()> {
        let mut events = Vec::with_capacity(2);
        if dx != 0 {
            events.push(raw_event(
                EventType::RELATIVE,
                RelativeAxisCode::REL_X.0,
                dx,
            ));
        }
        if dy != 0 {
            events.push(raw_event(
                EventType::RELATIVE,
                RelativeAxisCode::REL_Y.0,
                dy,
            ));
        }
        if events.is_empty() {
            return Ok(());
        }
        self.dev.emit(&events)
    }

    /// Vertical scroll: positive `ticks` = wheel down, negative = up.
    pub fn scroll_v(&mut self, ticks: i32) -> io::Result<()> {
        if ticks == 0 {
            return Ok(());
        }
        self.dev.emit(&[raw_event(
            EventType::RELATIVE,
            RelativeAxisCode::REL_WHEEL.0,
            ticks,
        )])
    }

    /// Horizontal scroll: positive `ticks` = wheel right, negative = left.
    pub fn scroll_h(&mut self, ticks: i32) -> io::Result<()> {
        if ticks == 0 {
            return Ok(());
        }
        self.dev.emit(&[raw_event(
            EventType::RELATIVE,
            RelativeAxisCode::REL_HWHEEL.0,
            ticks,
        )])
    }

    /// Release every button. Used on disable / shutdown so the pointer
    /// never ends up stuck mid-drag.
    pub fn release_all(&mut self) -> io::Result<()> {
        self.dev.emit(&[
            raw_event(EventType::KEY, KeyCode::BTN_LEFT.0, 0),
            raw_event(EventType::KEY, KeyCode::BTN_RIGHT.0, 0),
            raw_event(EventType::KEY, KeyCode::BTN_MIDDLE.0, 0),
        ])
    }
}

/// Build an `InputEvent` from raw `(type, code, value)` triple.
fn raw_event(ty: EventType, code: u16, value: i32) -> InputEvent {
    InputEvent::new(ty.0, code, value)
}
