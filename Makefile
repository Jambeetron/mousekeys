SHELL := /bin/bash

# --- configuration -----------------------------------------------------------
PREFIX    ?= $(HOME)/.local
BINDIR     = $(PREFIX)/bin
UNITDIR    = $(HOME)/.config/systemd/user
TARGET    := release

# --- targets ----------------------------------------------------------------
.PHONY: build release install uninstall reload enable disable start stop restart logs status clean audit fmt clippy check-dev

build:
	cargo build

release:
	cargo build --release

check-dev: fmt clippy
	cargo build --release

fmt:
	cargo fmt --check

clippy:
	cargo clippy --all-targets -- -D warnings

# Install the built binary and the systemd user unit.
install: release
	@install -Dm755 target/release/mousekeys "$(DESTDIR)$(BINDIR)/mousekeys"
	@install -Dm644 assets/mousekeys.service "$(DESTDIR)$(UNITDIR)/mousekeys.service"
	@systemctl --user daemon-reload
	@echo "Installed to $(BINDIR)/mousekeys"
	@echo "Enable with:  make enable"
	@echo "Remember: your user must be in the 'input' and 'uinput' groups."
	@echo "            sudo usermod -aG input,uinput $$USER   (then re-login)"

enable:
	@systemctl --user enable --now mousekeys.service
	@echo "Mouse Keys enabled and started."

disable:
	@systemctl --user disable --now mousekeys.service
	@echo "Mouse Keys disabled and stopped."

start:
	@systemctl --user start mousekeys.service

stop:
	@systemctl --user stop mousekeys.service

restart:
	@systemctl --user restart mousekeys.service

reload:
	@systemctl --user daemon-reload
	@systemctl --user restart mousekeys.service

logs:
	@journalctl --user -u mousekeys.service -f

status:
	@systemctl --user status mousekeys.service --no-pager

uninstall:
	@systemctl --user disable --now mousekeys.service 2>/dev/null || true
	@rm -f "$(DESTDIR)$(BINDIR)/mousekeys" "$(DESTDIR)$(UNITDIR)/mousekeys.service"
	@systemctl --user daemon-reload
	@echo "Uninstalled."

audit:
	@command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit
	@cargo audit

clean:
	cargo clean