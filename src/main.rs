// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Mouse Keys daemon entry point.
//!
//! A WM-independent daemon that turns the numeric keypad into a mouse
//! controller. It reads keyboard events directly from `/dev/input/event*`
//! via evdev and writes synthetic mouse events through `/dev/uinput`.
//!
//! The daemon **always grabs** every keyboard it manages (like `keyd` and
//! `kanata`). The compositor never sees the physical keyboard directly —
//! it only sees our virtual keyboard. This makes Super+H toggle possible
//! without leaving Super "stuck" in the compositor, because all Super
//! press/release events come from the same virtual device.
//!
//! Non-numpad keys (arrows, editing keys, function keys, modifiers) are
//! re-emitted verbatim on the virtual keyboard, so normal typing and all
//! keybinds keep working. Only the dedicated `KEY_KP*` codes are consumed
//! for mouse actions when enabled.
//!
//! Super + H toggles whether the numpad is consumed (default off). Numpad
//! Enter cycles between movement mode (default) and scroll mode.

use std::io;
use std::os::fd::AsFd;
use std::process::ExitCode;

use clap::Parser;
use evdev::{EventType, InputEvent, KeyCode};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use tracing::{error, info, warn};

mod input;
mod keys;
mod mouse;
mod notify;
mod reinject;
mod state;

use input::KeyboardSet;
use keys::{Action, Mode};
use mouse::Mouse;
use reinject::Keyboard;
use state::State;

/// Command-line configuration.
#[derive(Debug, Parser)]
#[command(
    name = "mousekeys",
    version,
    about = "WM-independent Mouse Keys daemon"
)]
struct Cli {
    /// Initial motion speed in pixels per movement event (4..=200).
    #[arg(long, default_value_t = state::SPEED_INITIAL)]
    speed: u32,
    /// Start with mouse-keys already enabled.
    #[arg(long, default_value_t = false)]
    enabled: bool,
}

fn main() -> ExitCode {
    init_tracing();
    if let Err(e) = real_main() {
        error!("fatal: {e:#}");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("mousekeys=info,warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

fn real_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let (mut keyboards, keys_union) = KeyboardSet::discover()?;
    info!(
        "managing {} keyboard device(s): {:?}",
        keyboards.names().len(),
        keyboards.names()
    );

    // Always grab — the compositor only sees our virtual keyboard.
    keyboards.grab_all()?;
    info!("grabbed all keyboards (exclusive, permanent)");

    let mut mouse = Mouse::new()?;
    let mut reinject = Keyboard::new(keys_union)?;
    info!("virtual mouse + keyboard ready");

    let state = State::default();
    state.set_enabled(cli.enabled);
    if (state::SPEED_BOUNDS.0..=state::SPEED_BOUNDS.1).contains(&cli.speed) {
        state.set_speed(cli.speed);
    } else {
        warn!("--speed {} out of range, keeping default", cli.speed);
    }

    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGINT);
    mask.add(Signal::SIGHUP);
    mask.thread_block().map_err(io::Error::from)?;
    let sfd = SignalFd::with_flags(&mask, SfdFlags::SFD_NONBLOCK).map_err(io::Error::from)?;

    info!(
        "ready (initial: enabled={}, mode=movement, speed={}, scroll-speed={})",
        state.enabled(),
        state.speed(),
        state.scroll_speed()
    );
    info!("toggle with Super+H");

    if let Err(e) = run_loop(&mut keyboards, &sfd, &mut mouse, &mut reinject, &state) {
        error!("run loop ended with error: {e:#}");
    }

    info!("shutting down");
    let _ = mouse.release_all();
    keyboards.ungrab_all();
    Ok(())
}

/// Super+H chord tracking.
struct ChordState {
    super_l: bool,
    super_r: bool,
    /// True when Super was consumed by Super+H; the real Super release
    /// must be swallowed (we emitted a synthetic one already).
    super_consumed: bool,
    /// True when H was consumed by Super+H; its release must be swallowed.
    h_swallowed: bool,
}

impl ChordState {
    fn new() -> Self {
        Self {
            super_l: false,
            super_r: false,
            super_consumed: false,
            h_swallowed: false,
        }
    }
}

fn run_loop(
    keyboards: &mut KeyboardSet,
    sfd: &SignalFd,
    mouse: &mut Mouse,
    reinject: &mut Keyboard,
    state: &State,
) -> anyhow::Result<()> {
    let mut ch = ChordState::new();

    loop {
        let ready_keyboard_indices: Vec<usize> = {
            let mut pfds: Vec<PollFd> = Vec::with_capacity(1 + keyboards.devices.len());
            pfds.push(PollFd::new(sfd.as_fd(), PollFlags::POLLIN));
            for kd in &keyboards.devices {
                pfds.push(PollFd::new(kd.dev.as_fd(), PollFlags::POLLIN));
            }

            match poll(&mut pfds, PollTimeout::NONE) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => return Err(io::Error::from(e).into()),
            }

            if pfds[0]
                .revents()
                .is_some_and(|f| f.contains(PollFlags::POLLIN))
                && sfd.read_signal().is_ok()
            {
                info!("signal received, shutting down");
                return Ok(());
            }

            pfds[1..]
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    p.revents()
                        .is_some_and(|f| f.contains(PollFlags::POLLIN))
                        .then_some(i)
                })
                .collect()
        };

        for idx in ready_keyboard_indices {
            let events = match keyboards.read_events(idx) {
                Ok(e) => e,
                Err(e) => {
                    warn!("read error on {}: {}", keyboards.devices[idx].name, e);
                    continue;
                }
            };
            for ev in events {
                if let Err(e) = handle_event(&ev, &mut ch, state, mouse, reinject) {
                    warn!("event handling error: {e}");
                }
            }
        }
    }
}

fn handle_event(
    ev: &InputEvent,
    ch: &mut ChordState,
    state: &State,
    mouse: &mut Mouse,
    reinject: &mut Keyboard,
) -> io::Result<()> {
    if ev.event_type() != EventType::KEY {
        return Ok(());
    }
    let key = KeyCode(ev.code());
    let value = ev.value();
    let enabled = state.enabled();

    // ── Super modifier tracking ──────────────────────────────────────────
    // Super is forwarded immediately — no buffering. This keeps Super+LMB
    // drag, Super+RMB resize, and lone-Super search toggle working.
    if key == KeyCode::KEY_LEFTMETA || key == KeyCode::KEY_RIGHTMETA {
        let is_left = key == KeyCode::KEY_LEFTMETA;
        if value == 1 {
            if is_left {
                ch.super_l = true;
            } else {
                ch.super_r = true;
            }
            reinject.forward(ev)?;
            return Ok(());
        } else if value == 0 {
            if is_left {
                ch.super_l = false;
            } else {
                ch.super_r = false;
            }
            if ch.super_consumed {
                ch.super_consumed = false;
                return Ok(());
            }
            reinject.forward(ev)?;
            return Ok(());
        } else {
            reinject.forward(ev)?;
            return Ok(());
        }
    }

    // ── Super + H toggle chord ───────────────────────────────────────────
    if key == KeyCode::KEY_H {
        let chord = ch.super_l || ch.super_r;
        match value {
            1 if chord => {
                // Super+H detected. The compositor saw Super from our
                // virtual keyboard. We need to emit a synthetic Super
                // release to cancel the Quickshell searchToggleRelease
                // timer. But first, tell Quickshell to not trigger on
                // release — otherwise the synthetic release itself would
                // toggle the overview.
                fire_quickshell_interrupt();

                // Emit synthetic Super release on the same virtual keyboard
                // that the press came from → per-device tracking is
                // consistent → compositor sees Super go up cleanly.
                if ch.super_l {
                    reinject.forward(&synthetic(KeyCode::KEY_LEFTMETA, 0))?;
                }
                if ch.super_r {
                    reinject.forward(&synthetic(KeyCode::KEY_RIGHTMETA, 0))?;
                }
                ch.super_consumed = true;
                ch.h_swallowed = true;

                let now = !enabled;
                state.set_enabled(now);
                if now {
                    info!("mouse keys ENABLED");
                    notify::toggle(true);
                } else {
                    if state.left_held() {
                        mouse.release_left()?;
                        state.set_left_held(false);
                    }
                    info!("mouse keys DISABLED");
                    notify::toggle(false);
                }
                return Ok(());
            }
            0 if ch.h_swallowed => {
                ch.h_swallowed = false;
                return Ok(());
            }
            2 if ch.h_swallowed => return Ok(()),
            _ => {
                reinject.forward(ev)?;
                return Ok(());
            }
        }
    }

    // ── Enabled: maybe consume a numpad key ───────────────────────────────
    if enabled && keys::is_consumed(key) {
        if let Some(action) = keys::resolve(key, state.mode()) {
            apply_action(&action, value, state, mouse)?;
        }
        return Ok(());
    }

    // ── Default: forward via virtual keyboard ──────────────────────────────
    reinject.forward(ev)
}

/// Best-effort: tell Quickshell to not toggle the overview on the upcoming
/// Super release. Uses direct exec (no shell). If `qs` is not found or
/// Quickshell is not running, the call silently fails — the synthetic
/// Super release might then toggle the overview, but the daemon still
/// functions correctly.
fn fire_quickshell_interrupt() {
    // Synchronous: we must finish before emitting the synthetic Super
    // release, otherwise the release arrives at Hyprland before the flag
    // is cleared and the overview still toggles.
    let _ = std::process::Command::new("qs")
        .args([
            "-c",
            "ii",
            "ipc",
            "call",
            "search",
            "toggleReleaseInterrupt",
        ])
        .status();
}

/// Build a synthetic key event (value 0 = release, 1 = press).
fn synthetic(key: KeyCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, key.0, value)
}

fn apply_action(action: &Action, value: i32, state: &State, mouse: &mut Mouse) -> io::Result<()> {
    let press = value == 1;
    let repeat = value == 2;
    match action {
        Action::Move { dx, dy } if press || repeat => match state.mode() {
            Mode::Movement => {
                let speed = state.speed();
                mouse.move_rel(dx * speed, dy * speed)?;
            }
            Mode::Scroll => {
                let speed = state.scroll_speed();
                if *dy != 0 {
                    mouse.scroll_v(-dy * speed)?;
                }
                if *dx != 0 {
                    mouse.scroll_h(*dx * speed)?;
                }
            }
        },
        Action::LeftClick if press => mouse.left_click()?,
        Action::MiddleClick if press => mouse.middle_click()?,
        Action::RightClick if press => mouse.right_click()?,
        Action::HoldLeft if press => {
            state.set_left_held(true);
            mouse.hold_left()?;
        }
        Action::Release if press => {
            state.set_left_held(false);
            mouse.release_left()?;
        }
        Action::SpeedUp if press => match state.mode() {
            Mode::Movement => {
                let n = state.speed_up();
                info!("speed -> {n}");
                notify::show("Mouse Keys", &format!("speed: {n}"));
            }
            Mode::Scroll => {
                let n = state.scroll_speed_up();
                info!("scroll speed -> {n}");
                notify::show("Mouse Keys", &format!("scroll speed: {n}"));
            }
        },
        Action::SpeedDown if press => match state.mode() {
            Mode::Movement => {
                let n = state.speed_down();
                info!("speed -> {n}");
                notify::show("Mouse Keys", &format!("speed: {n}"));
            }
            Mode::Scroll => {
                let n = state.scroll_speed_down();
                info!("scroll speed -> {n}");
                notify::show("Mouse Keys", &format!("scroll speed: {n}"));
            }
        },
        Action::CycleMode if press => {
            let m = state.toggle_mode();
            info!("mode -> {}", m.as_str());
            notify::mode_change(m);
        }
        _ => {}
    }
    Ok(())
}
