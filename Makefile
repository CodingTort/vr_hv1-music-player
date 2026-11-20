# vr_hv1's Music Player: the desktop app `lavanda`, the terminal client
# `vrmusic-tui`, and `vrmusic`, which runs the terminal client in tmux.
#
#   make                 build both (release)
#   make install         build, then install for your user under ~/.local
#   make uninstall       remove what `make install` put in place
#   make test            run the test suite
#
# System-wide: build as yourself, then copy the files as root:
#   make && sudo make install-files PREFIX=/usr/local

PREFIX  ?= $(HOME)/.local
BINDIR  := $(DESTDIR)$(PREFIX)/bin
DATADIR := $(DESTDIR)$(PREFIX)/share
ICONDIR := $(DATADIR)/icons/hicolor

.PHONY: all build install install-files uninstall test run run-tui clean

all: build

build:
	cargo build --release --locked

install: build
	@$(MAKE) --no-print-directory install-files

install-files:
	@test -x target/release/lavanda -a -x target/release/vrmusic-tui || { echo "the programs aren't built: run 'make' first"; exit 1; }
	install -Dm755 target/release/lavanda $(BINDIR)/lavanda
	install -Dm755 target/release/vrmusic-tui $(BINDIR)/vrmusic-tui
	install -Dm755 scripts/vrmusic $(BINDIR)/vrmusic
	install -Dm644 assets/lavanda.desktop $(DATADIR)/applications/lavanda.desktop
	install -Dm644 assets/tux.svg $(ICONDIR)/scalable/apps/tort-music-player.svg
	install -Dm644 assets/tux.png $(ICONDIR)/256x256/apps/tort-music-player.png
	@echo "Installed. Launch \"vr_hv1's Music Player\" from your app launcher, or run: lavanda (vrmusic in a terminal)"

uninstall:
	rm -f $(BINDIR)/lavanda $(BINDIR)/vrmusic-tui $(BINDIR)/vrmusic
	rm -f $(DATADIR)/applications/lavanda.desktop
	rm -f $(ICONDIR)/scalable/apps/tort-music-player.svg
	rm -f $(ICONDIR)/256x256/apps/tort-music-player.png

test:
	cargo test --locked

run:
	cargo run --release --locked --bin lavanda

run-tui:
	cargo run --release --locked --bin vrmusic-tui

clean:
	cargo clean
