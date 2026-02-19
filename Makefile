SHELL := /usr/bin/env bash

CARGO ?= cargo
PRX_CONFIG ?= ./Prx.toml

.DEFAULT_GOAL := help

.PHONY: help run run-release build build-release check fmt fmt-check clippy test audit gate ci clean

help: ## Show available targets
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "%-14s %s\n", $$1, $$2}' $(MAKEFILE_LIST) | sort

run: ## Run proxy in debug mode
	PRX_CONFIG=$(PRX_CONFIG) $(CARGO) run

run-release: ## Run proxy in release mode
	PRX_CONFIG=$(PRX_CONFIG) $(CARGO) run --release

build: ## Build debug binary
	$(CARGO) build

build-release: ## Build release binary
	$(CARGO) build --release

check: ## Type-check the project
	$(CARGO) check

fmt: ## Format Rust code
	$(CARGO) fmt --all

fmt-check: ## Verify formatting
	$(CARGO) fmt --all --check

clippy: ## Run clippy with warnings as errors
	$(CARGO) clippy --all-targets -- -D warnings

test: ## Run all tests
	$(CARGO) test --all-targets -- --test-threads=1

audit: ## Run cargo audit (installs cargo-audit if missing)
	@if ! command -v cargo-audit >/dev/null 2>&1; then \
		$(CARGO) install cargo-audit --locked; \
	fi
	$(CARGO) audit

gate: ## Run full release gate
	bash scripts/release-gate.sh

ci: gate ## Alias for gate

clean: ## Clean build artifacts
	$(CARGO) clean
