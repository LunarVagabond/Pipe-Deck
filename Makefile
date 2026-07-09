.PHONY: help install start dev dev-frontend build build-frontend build-rust check test clean preview

NPM ?= npm
CARGO ?= cargo
TAURI_DIR := src-tauri

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

build: ## Build production desktop bundles (.deb, .rpm, AppImage, binary)
	$(NPM) run tauri build

build-frontend: ## Type-check and build the Vue frontend
	$(NPM) run build

build-rust: ## Compile the Rust backend (debug)
	$(CARGO) build --manifest-path $(TAURI_DIR)/Cargo.toml

check: ## Run frontend and Rust checks without producing bundles
	$(NPM) run build
	$(CARGO) check --manifest-path $(TAURI_DIR)/Cargo.toml

test: ## Run Rust tests
	$(CARGO) test --manifest-path $(TAURI_DIR)/Cargo.toml

preview: ## Preview the built frontend assets
	$(NPM) run preview

clean: ## Remove build artifacts
	rm -rf dist node_modules/.vite
	$(CARGO) clean --manifest-path $(TAURI_DIR)/Cargo.toml
