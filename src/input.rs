// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Discovery and management of physical keyboard evdev devices.
//!
//! The daemon opens keyboard devices in non-blocking mode and reads events
//! from them. While mouse-keys is **disabled** the devices are left
//! un-grabbed — events flow naturally to the compositor and we only
//! passively observe for the Super+H toggle chord. When **enabled** the
//! devices are grabbed exclusively (`EVIOCGRAB`) so numpad keys can be
//! consumed; any event we do not interpret is re-emitted on a virtual
//! keyboard (see [`crate::reinject`]).

use std::io;
use std::os::fd::AsFd;
use std::path::PathBuf;

use evdev::{AttributeSet, EventType, KeyCode, RelativeAxisCode, raw_stream::RawDevice};
use nix::fcntl::{FcntlArg, OFlag, fcntl};

/// A physical keyboard we are supervising.
pub struct KeyboardDevice {
    pub dev: RawDevice,
    pub path: PathBuf,
    pub name: String,
}

/// Everything we grabbed at startup; released on `Drop`.
pub struct KeyboardSet {
    pub devices: Vec<KeyboardDevice>,
}

impl KeyboardSet {
    /// Enumerate `/dev/input/event*`, keep the ones that look like
    /// keyboards, put their fds in non-blocking mode, and return the set
    /// plus the union of all supported key codes (used to build the
    /// virtual re-injection keyboard). Devices are NOT grabbed yet.
    pub fn discover() -> io::Result<(Self, AttributeSet<KeyCode>)> {
        let mut devices = Vec::new();
        let mut merged: AttributeSet<KeyCode> = AttributeSet::new();

        for (path, dev) in evdev::raw_stream::enumerate() {
            if !is_keyboard(&dev) {
                continue;
            }
            let name = dev.name().unwrap_or("").to_string();
            // Skip our own virtual devices (in case a previous instance
            // hasn't fully torn down yet, or uinput nodes persist briefly).
            if name.starts_with("mousekeys virtual") {
                continue;
            }
            let _ = fcntl(dev.as_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK));
            if let Some(keys) = dev.supported_keys() {
                for k in keys.iter() {
                    merged.insert(k);
                }
            }
            devices.push(KeyboardDevice { dev, path, name });
        }

        if devices.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no keyboard evdev devices found under /dev/input",
            ));
        }

        Ok((Self { devices }, merged))
    }

    pub fn names(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.name.clone()).collect()
    }

    /// Grab all devices exclusively. Called when mouse-keys is enabled.
    /// May fail if another process already holds a grab.
    pub fn grab_all(&mut self) -> io::Result<()> {
        for d in &mut self.devices {
            if let Err(e) = d.dev.grab() {
                return Err(io::Error::other(format!(
                    "EVIOCGRAB failed on {}: {e}",
                    d.path.display()
                )));
            }
        }
        Ok(())
    }

    /// Release all grabs. Called when mouse-keys is disabled and on
    /// shutdown (also via `Drop`).
    pub fn ungrab_all(&mut self) {
        for d in &mut self.devices {
            let _ = d.dev.ungrab();
        }
    }

    /// Drain pending events from device `idx`. Returns an empty vector on
    /// `EAGAIN` (no events after all).
    pub fn read_events(&mut self, idx: usize) -> io::Result<Vec<evdev::InputEvent>> {
        let dev = &mut self.devices[idx].dev;
        match dev.fetch_events() {
            Ok(iter) => Ok(iter.collect()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }
}

impl Drop for KeyboardSet {
    fn drop(&mut self) {
        self.ungrab_all();
    }
}

/// Heuristic: this is a keyboard (not a mouse with buttons, not a tablet).
/// Has `EV_KEY` with typing keys, and lacks relative pointer axes.
fn is_keyboard(dev: &RawDevice) -> bool {
    let ev = dev.supported_events();
    if !ev.contains(EventType::KEY) {
        return false;
    }
    let Some(keys) = dev.supported_keys() else {
        return false;
    };
    let has_alpha = keys.contains(KeyCode::KEY_ENTER) || keys.contains(KeyCode::KEY_SPACE);
    if !has_alpha {
        return false;
    }
    if let Some(rel) = dev.supported_relative_axes() {
        if rel.contains(RelativeAxisCode::REL_X) || rel.contains(RelativeAxisCode::REL_Y) {
            return false;
        }
    }
    true
}
