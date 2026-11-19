# Tort's Music Player (vr_hv1's music client)
#
#   make                 build (release)
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

.PHONY: all build install install-files uninstall test run clean

all: build

build:
	cargo build --release --locked

install: build
	@$(MAKE) --no-print-directory install-files

install-files:
	@test -x target/release/lavanda || { echo "target/release/lavanda is missing: run 'make' first"; exit 1; }
	install -Dm755 target/release/lavanda $(BINDIR)/lavanda
	install -Dm644 assets/lavanda.desktop $(DATADIR)/applications/lavanda.desktop
	install -Dm644 assets/tux.svg $(ICONDIR)/scalable/apps/tort-music-player.svg
	install -Dm644 assets/tux.png $(ICONDIR)/256x256/apps/tort-music-player.png
	@echo "Installed. Launch \"Tort's Music Player\" from your app launcher, or run: lavanda"

uninstall:
	rm -f $(BINDIR)/lavanda
	rm -f $(DATADIR)/applications/lavanda.desktop
	rm -f $(ICONDIR)/scalable/apps/tort-music-player.svg
	rm -f $(ICONDIR)/256x256/apps/tort-music-player.png

test:
	cargo test --locked

run:
	cargo run --release --locked

clean:
	cargo clean
