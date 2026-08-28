# Mouse Keys

A **window-manager-independent** Linux daemon that turns the numeric keypad
into a mouse controller. It reads keyboard events directly from
`/dev/input/event*` (via evdev) and writes synthetic mouse events through
`/dev/uinput`. There is **no dependency on a specific Wayland compositor,
X server, desktop environment, or GUI toolkit** — anywhere evdev and uinput
work, this works.

It is written in Rust (edition 2024) and designed with security in mind:
no networking, no filesystem writes, no shell-out, no C dependencies beyond
the crates pulled in by `evdev`, `nix`, `clap`, and `tracing`, and a
hardened systemd unit.

---

## Behaviour

While the daemon is running it **exclusively grabs every keyboard device**
it manages, so numpad keys never leak to other apps. Anything it does not
interpret is re-emitted on a virtual keyboard, so normal typing keeps
working.

| Key                 | Movement mode        | Scroll mode        | Always               |
|---------------------|----------------------|--------------------|----------------------|
| `8` / `Up`          | move up              | wheel up           |                      |
| `2` / `Down`        | move down            | wheel down         |                      |
| `4` / `Left`        | move left            | horizontal wheel L |                      |
| `6` / `Right`       | move right           | horizontal wheel R |                      |
| `7` / `Home`        | diagonal up-left     | (swallowed, no-op) |                      |
| `9` / `PgUp`        | diagonal up-right    | (swallowed, no-op) |                      |
| `1` / `End`         | diagonal down-left   | (swallowed, no-op) |                      |
| `3` / `PgDn`        | diagonal down-right   | (swallowed, no-op) |                      |
| `5`                 | left click           | left click         |                      |
| `/`                 | middle click         | middle click       |                      |
| `*`                 | right click          | right click        |                      |
| `+`                 | increase speed       | increase scroll speed |                   |
| `-`                 | decrease speed       | decrease scroll speed |                   |
| `0` / `Insert`      | hold left button     | hold left button   |                      |
| `.` / `Delete`      | release button       | release button     |                      |
| `Enter` (numpad)    | switch to scroll     | switch to movement |                      |
| `Super + H`         |                      |                    | toggle feature on/off|

* NumLock is irrelevant: both the NumLock-ON keypad codes and their
  NumLock-OFF navigation equivalents are recognised.
* Default speed is **20 px/event**, adjustable with `+`/`-` in steps of 4,
  clamped to `[4, 200]`. In scroll mode the same `+`/`-` keys adjust scroll
  speed (wheel ticks per event, default 1, clamped to `[1, 20]`).
* **Desktop notifications** are sent on toggle (ACTIVATED/DEACTIVATED), on
  every mode change (Numpad Enter), and on each speed/scroll-speed change.
  They use the D-Bus `org.freedesktop.Notifications` interface via the
  pure-Rust `notify-rust` crate — no `notify-send` shell-out.
* `0` holds the left button down (drag); `.` releases it (drop).
* While mouse-keys is **off**, the numpad behaves exactly as a normal
  keypad (events are re-injected); only `Super + H` is observed to turn
  it on.
* Turning the feature off automatically releases any held button so the
  pointer is never stuck mid-drag.

### Known limitation

Because the daemon grabs the keyboard and re-injects non-interpreted keys,
a tap of `Super` alone is delivered to the compositor as a lone Super
press/release. Some compositors bind a lone Super tap to launch an
application launcher; you may see that trigger when toggling with
`Super + H`. This is an inherent consequence of using evdev+uinput instead
of a compositor-specific keybind, and is the same trade-off made by tools
like `keyd` and `kanata`.

---

## Requirements

* Linux with `/dev/uinput` available (the `uinput` kernel module).
* `evdev`-supported keyboards exposed under `/dev/input/event*`.
* Your user must be able to read `/dev/input/event*` and read/write
  `/dev/uinput`. On most distributions this means membership in the
  `input` and `uinput` groups:

  ```
  sudo usermod -aG input,uinput "$USER"
  ```

  Log out and back in for the group change to take effect. Some
  distributions do not ship a `uinput` group; in that case make sure
  `/dev/uinput` is group-writable by a group your user belongs to (e.g.
  via a udev rule), or run the binary with the necessary capabilities.

Check that `/dev/uinput` exists and is writable by your user:

```
ls -l /dev/uinput
test -w /dev/uinput && echo OK
```

---

## Build

```
cargo build --release
```

The optimised binary is `target/release/mousekeys`.

Run directly for a quick test:

```
./target/release/mousekeys --help
RUST_LOG=debug ./target/release/mousekeys --enabled
```
Toggle with `Super + H`. Stop with `Ctrl+C` or `kill -TERM <pid>`.

### Verifying dependencies for known supply-chain issues

```
make audit        # installs cargo-audit if missing, then `cargo audit`
```

---

## Install as a systemd user service

The included Makefile installs the binary to `~/.local/bin/mousekeys` and
the unit to `~/.config/systemd/user/mousekeys.service`, then enables and
starts it on the graphical session:

```
make install      # build release + install binary + unit, daemon-reload
make enable       # systemctl --user enable --now mousekeys.service
```

The service is ordered after `graphical-session.target` so it starts
automatically on login.

Inspect logs:

```
make logs         # journalctl --user -u mousekeys -f
make status       # systemctl --user status mousekeys
```

Restart after an update:

```
make release && make restart
```

Uninstall completely:

```
make uninstall
```

### Manual install (without the Makefile)

```
cargo build --release
install -Dm755 target/release/mousekeys ~/.local/bin/mousekeys
install -Dm644 assets/mousekeys.service ~/.config/systemd/user/mousekeys.service
systemctl --user daemon-reload
systemctl --user enable --now mousekeys.service
journalctl --user -u mousekeys -f
```

---

## CLI options

```
Usage: mousekeys [OPTIONS]

Options:
      --speed <SPEED>  Initial motion speed in pixels per movement event (4..=200) [default: 20]
      --enabled        Start with mouse-keys already enabled
  -h, --help           Print help
  -V, --version        Print version
```

Logging is controlled with the `RUST_LOG` environment variable
(`mousekeys=info,warn` by default).

---

## Security notes

* **No network access.** Nothing in the dependency tree opens a socket.
* **No filesystem writes** beyond journald capturing stderr.
* **No shell-out**, no `system()`, no `exec()`; all input/output is via
  kernel device fds and ioctls.
* **grab/grab-release discipline.** The daemon releases every grab via
  `EVIOCGRAB` on the way out (through `Drop` impls and clean shutdown on
  SIGTERM/SIGINT/SIGHUP, drained via a `signalfd`). The systemd unit sets
  `Restart=on-failure` with a 1-second back-off, so a crash never leaves
  the keyboard grabbed for long.
* **Hardened systemd unit:** `NoNewPrivileges`, `PrivateTmp`,
  `ProtectSystem=strict`, `LockPersonality`, `MemoryDenyWriteExecute`,
  a restrictive `SystemCallFilter`, and `SystemCallArchitectures=native`.
* **Bounded state.** All mutable runtime values are atomic and clamped
  (`speed` is validated to `[4, 200]`); every evdev event is matched
  before any action is taken; no unexpected event types are written to
  uinput.
* **Pinned dependencies, auditable build.** Run `make audit` for a
  supply-chain scan.

`unsafe` is confined to the `evdev` and `nix` crate ioctl wrappers.

---

## Troubleshooting

* **`fatal: no keyboard evdev devices found under /dev/input`** — no
  device passed the keyboard heuristic. Run `cat /proc/bus/input/devices`
  and verify there is an entry with `KEY=...` containing letter keys and
  no `REL=...` axes. Laptop lid sensor devices and button-only devices are
  filtered out on purpose.
* **`fatal: Permission denied` opening `/dev/uinput`** — your user is
  not in the `uinput` group (or `/dev/uinput` is not group-writable for
  you). Add yourself to the group and re-login.
* **Keys feel "stuck" after a crash** — run `sudo systemctl --user
  restart mousekeys` or just kill any leftover `mousekeys` process; the
  new instance will re-grab and the kernel releases the old grab when
  the crashed fd is closed.
* **Lone Super taps open a launcher** — see *Known limitation* above.

---

## Layout

```
src/
  main.rs      arg parsing, logging, lifecycle, single-threaded event loop, signals
  input.rs     /dev/input discovery, keyboard detection, grab/ungrab
  reinject.rs  virtual uinput keyboard (forward non-interpreted keys)
  mouse.rs     virtual uinput mouse (motion, clicks, scroll, hold/release)
  keys.rs      keycode -> action table, NumLock-agnostic
  state.rs     shared atomic state: enabled, mode, speed, held button
assets/
  mousekeys.service  systemd user unit
Makefile       build / install / enable / logs / audit / clean
```

## License

GNU Affero General Public License v3.0 or later (AGPLv3+). See [LICENSE](LICENSE).

Copyright (C) 2026 Jambeetron.