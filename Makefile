.PHONY: help install start dev dev-frontend build build-daemon build-daemon-dev build-cli build-frontend build-rust check test clean preview flatpak smoke

NPM ?= npm
CARGO ?= cargo
TAURI_DIR := src-tauri
HOST_TRIPLE := $(shell rustc -vV | sed -n 's/^host: //p')
export CARGO_TARGET_DIR := $(TAURI_DIR)/target
DAEMON_BIN_DEBUG := $(CARGO_TARGET_DIR)/debug/pipe-deck-daemon
DAEMON_BIN_RELEASE := $(CARGO_TARGET_DIR)/release/pipe-deck-daemon
CLI_BIN_DEBUG := $(CARGO_TARGET_DIR)/debug/pipe-deck-cli

.DEFAULT_GOAL := help

help: ## Show available commands
	@printf "Pipe Deck development commands\n\n"
	@grep -E '^[a-zA-Z0-9_-]+:.*##' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

install: ## Install frontend dependencies
	$(NPM) install

start: dev ## Run the desktop app in development mode

dev: ## Run the desktop app in development mode (Tauri + Vite)
	rm -rf node_modules/.vite
	$(NPM) run tauri dev

dev-frontend: ## Run only the Vite frontend dev server
	rm -rf node_modules/.vite
	$(NPM) run dev

ensure-daemon-stub: ## Placeholder daemon binary for Tauri externalBin checks
	mkdir -p $(TAURI_DIR)/bin
	@test -f $(TAURI_DIR)/bin/pipe-deck-daemon-$(HOST_TRIPLE) || \
	  (printf '#!/bin/sh\nexit 0\n' > $(TAURI_DIR)/bin/pipe-deck-daemon-$(HOST_TRIPLE) && \
	  chmod +x $(TAURI_DIR)/bin/pipe-deck-daemon-$(HOST_TRIPLE))

build-daemon: ensure-daemon-stub ## Build the headless restore daemon binary (release)
	$(CARGO) build --release --manifest-path $(TAURI_DIR)/Cargo.toml --bin pipe-deck-daemon
	cp $(DAEMON_BIN_RELEASE) $(TAURI_DIR)/bin/pipe-deck-daemon-$(HOST_TRIPLE)

build-daemon-dev: ensure-daemon-stub ## Build daemon binary for dev/test (debug)
	$(CARGO) build --manifest-path $(TAURI_DIR)/Cargo.toml --bin pipe-deck-daemon
	cp $(DAEMON_BIN_DEBUG) $(TAURI_DIR)/bin/pipe-deck-daemon-$(HOST_TRIPLE)

build-cli: build-daemon-dev ## Build pipe-deck CLI binary (debug)
	$(CARGO) build --manifest-path $(TAURI_DIR)/Cargo.toml --bin pipe-deck-cli

build: build-daemon ## Build production desktop bundles (.deb, .rpm, AppImage, binary)
	$(NPM) run tauri build

build-frontend: ## Type-check and build the Vue frontend
	$(NPM) run build

build-rust: build-daemon-dev build-cli ## Compile the Rust backend (debug)
	$(CARGO) build --manifest-path $(TAURI_DIR)/Cargo.toml

check: build-daemon-dev build-cli ## Run frontend and Rust checks without producing bundles
	$(NPM) run build
	$(CARGO) check --manifest-path $(TAURI_DIR)/Cargo.toml

test: build-daemon-dev build-cli ## Run Rust tests
	$(CARGO) test --manifest-path $(TAURI_DIR)/Cargo.toml

preview: ## Preview the built frontend assets
	$(NPM) run preview

clean: ## Remove build artifacts
	rm -rf dist node_modules/.vite
	$(CARGO) clean --manifest-path $(TAURI_DIR)/Cargo.toml

flatpak: ## Build Flatpak package locally
	flatpak-builder --force-clean flatpak/build flatpak/com.pipedeck.PipeDeck.yml

smoke: ## Run install and compile smoke checks
	bash scripts/smoke-install.sh
