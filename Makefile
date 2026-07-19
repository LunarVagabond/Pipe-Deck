SHELL := /usr/bin/env bash

.PHONY: help install start dev dev-frontend build build-daemon build-daemon-dev build-cli build-frontend build-rust check test test-unit test-e2e clean preview flatpak smoke release release-checks

NPM ?= npm
CARGO ?= cargo
TAURI_DIR := src-tauri
HOST_TRIPLE := $(shell rustc -vV | sed -n 's/^host: //p')
export CARGO_TARGET_DIR := $(abspath $(TAURI_DIR)/target)
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

check: build-daemon-dev build-cli ## Run frontend type-check, frontend unit tests, and Rust checks without producing bundles
	$(NPM) run build
	$(NPM) run test:unit
	$(CARGO) check --manifest-path $(TAURI_DIR)/Cargo.toml

test: build-daemon-dev build-cli ## Run Rust tests
	$(CARGO) test --manifest-path $(TAURI_DIR)/Cargo.toml

test-unit: ## Run frontend Vitest unit tests (src/**/*.spec.ts)
	$(NPM) run test:unit

test-e2e: ## Run frontend Playwright component tests (src/e2e/, needs `npx playwright install chromium` once)
	$(NPM) run test:e2e

preview: ## Preview the built frontend assets
	$(NPM) run preview

clean: ## Remove build artifacts
	rm -rf dist node_modules/.vite
	$(CARGO) clean --manifest-path $(TAURI_DIR)/Cargo.toml

flatpak: ## Build Flatpak package locally
	flatpak-builder --force-clean flatpak/build flatpak/com.pipedeck.PipeDeck.yml

smoke: ## Run install and compile smoke checks
	bash scripts/smoke-install.sh

release-checks: ## Run the pre-release validation gate (type-check, frontend tests, cargo check, cargo test) standalone
	bash scripts/release-checks.sh

.PHONY: release
## Run pre-release checks, then update version files, commit, and create a release tag.
##
## Usage:
##   make release VER=0.2.0 TITLE="Some release title"
## - VER prompts if missing (shows current version). TITLE is optional.
## - Tag format:
##   - If TITLE is provided: v<VER>-<TITLE_SLUG>
##   - If TITLE is empty:    v<VER>
## - Runs scripts/release-checks.sh first; aborts with no changes made if it fails.
## - Bumps package.json, Cargo.toml, tauri.conf.json, and AppStream metainfo.
release:
	@set -euo pipefail; \
	echo "release: running pre-release checks (scripts/release-checks.sh)"; \
	if ! bash scripts/release-checks.sh; then \
		echo "release: pre-release checks failed — nothing was changed, fix and re-run 'make release'"; \
		exit 1; \
	fi; \
	ver="$(strip $(VER))"; \
	current_ver="$$(node -p 'require("./package.json").version' 2>/dev/null || true)"; \
	if [ -z "$$current_ver" ]; then \
		current_ver="$$(sed -nE 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' package.json | head -n 1)"; \
	fi; \
	if [ -z "$$ver" ]; then \
		if [ -n "$$current_ver" ]; then \
			read -r -p "Release version (current: $$current_ver): " ver; \
			if [ -z "$$ver" ]; then ver="$$current_ver"; fi; \
		else \
			read -r -p "Release version (X.Y.Z): " ver; \
		fi; \
	fi; \
	if ! [[ "$$ver" =~ ^[0-9]+\.[0-9]+\.[0-9]+$$ ]]; then \
		echo "release: invalid VER '$$ver' (expected X.Y.Z)"; \
		exit 1; \
	fi; \
	title="$(strip $(TITLE))"; \
	if [ -z "$$title" ]; then \
		read -r -p "Release title (optional): " title; \
	fi; \
	title_slug="$$(printf '%s' "$$title" | sed -E 's/[[:space:]]+/-/g; s/[^A-Za-z0-9._-]//g; s/^-+//; s/-+$$//')"; \
	tag_name="v$$ver"; \
	if [ -n "$$title_slug" ]; then \
		tag_name="$$tag_name-$$title_slug"; \
	fi; \
	echo "release: tag='$$tag_name' version='$$ver'"; \
	if git rev-parse -q --verify "refs/tags/$$tag_name" >/dev/null; then \
		echo "release: git tag '$$tag_name' already exists"; \
		exit 1; \
	fi; \
	npm version --no-git-tag-version --allow-same-version "$$ver" >/dev/null; \
	tmp_file="$$(mktemp)"; \
	awk -v ver="$$ver" '\
	BEGIN { in_package=0 } \
	$$0 == "[package]" { in_package=1; print; next } \
	in_package && $$0 ~ /^\[/ { in_package=0 } \
	in_package && $$0 ~ /^version[[:space:]]*=/ { $$0 = "version = \"" ver "\"" } \
	{ print }' src-tauri/Cargo.toml > "$$tmp_file"; \
	mv "$$tmp_file" src-tauri/Cargo.toml; \
	node -e 'const fs=require("fs"); const p="src-tauri/tauri.conf.json"; const j=JSON.parse(fs.readFileSync(p,"utf8")); j.version=process.argv[1]; fs.writeFileSync(p, JSON.stringify(j,null,2)+"\n");' "$$ver"; \
	release_date="$$(date -u +%Y-%m-%d)"; \
	metainfo="packaging/com.pipedeck.PipeDeck.metainfo.xml"; \
	if grep -q '<release version=' "$$metainfo"; then \
		sed -i "s|<release version=\"[^\"]*\" date=\"[^\"]*\" />|    <release version=\"$$ver\" date=\"$$release_date\" />|" "$$metainfo"; \
	else \
		sed -i "s|</releases>|    <release version=\"$$ver\" date=\"$$release_date\" />\n  </releases>|" "$$metainfo"; \
	fi; \
	(cd $(TAURI_DIR) && $(CARGO) check -q --manifest-path Cargo.toml) || true; \
	version_files="package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json packaging/com.pipedeck.PipeDeck.metainfo.xml"; \
	git add -- $$version_files; \
	if git diff --cached --quiet -- $$version_files; then \
		echo "release: no version changes to commit (continuing with tag)"; \
	else \
		git commit -m "Release $$tag_name"; \
	fi; \
	git tag -a "$$tag_name" -m "$$tag_name"; \
	echo "release done: $$tag_name"; \
	echo "Next: git push origin main --tags"
