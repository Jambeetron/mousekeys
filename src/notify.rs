// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Jambeetron
//
// mousekeys is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.

//! Desktop notifications via the D-Bus freedesktop.org spec.
//!
//! Uses `notify-rust` (pure Rust D-Bus client, no shell-out to
//! `notify-send`). Failures are logged but never propagated: notifications
//! are best-effort UX and must never break the daemon.

use tracing::warn;

const APP_NAME: &str = "Mouse Keys";

/// Show a notification. Never returns an error.
pub fn show(summary: &str, body: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .appname(APP_NAME)
        .summary(summary)
        .body(body)
        .timeout(notify_rust::Timeout::Milliseconds(1500))
        .show()
    {
        warn!("notification failed: {e}");
    }
}

/// Shorthand for the enable/disable toggle alerts.
pub fn toggle(enabled: bool) {
    if enabled {
        show("Mouse Keys", "ACTIVATED — press Super+H to deactivate");
    } else {
        show("Mouse Keys", "DEACTIVATED");
    }
}

/// Shorthand for the mode-cycle alert (Numpad Enter).
pub fn mode_change(mode: crate::keys::Mode) {
    show("Mouse Keys", &format!("mode: {}", mode.as_str()));
}
