# =============================================================================
# Makefile for the Flow Workspace
#
# This Makefile is designed to handle multiple binary packages.
# Most commands require you to specify the target package using `pkg=<name>`.
#
# Examples:
#   make run pkg=node
#   make build-release pkg=api-server
#   make test
# =============================================================================

# --- Configuration ---
# Default log level. Can be overridden, e.g., `make run pkg=node LOG_LEVEL=debug`
LOG_LEVEL ?= info

# Use a more descriptive variable for the package name.
# This will be set on the command line, e.g., `make run pkg=node`.
pkg ?= ""


# --- Core Recipes ---

.PHONY: run
run: check-pkg-defined
	@echo "--> Running package: $(pkg) (log level: $(LOG_LEVEL))"
	@RUST_LOG=$(LOG_LEVEL) cargo run --package $(pkg)

.PHONY: build
build: check-pkg-defined
	@echo "--> Building package: $(pkg) (dev)"
	@cargo build --package $(pkg)

.PHONY: build-release
build-release: check-pkg-defined
	@echo "--> Building package: $(pkg) for release (optimized)"
	@cargo build --release --package $(pkg)


# --- Workspace-Wide Recipes ---
# These do not require a `pkg` variable as they act on the whole workspace.

.PHONY: test
test:
	@echo "--> Running tests for all packages in the workspace, excluding doc-tests"
	@cargo test --workspace --tests

.PHONY: doc-test
doc-test:
	@echo "--> Running doc tests for all packages in the workspace"
	@cargo test --workspace --doc

.PHONY: check
check:
	@echo "--> Checking all packages in the workspace"
	@cargo check --workspace

.PHONY: clean
clean:
	@echo "--> Cleaning all build artifacts"
	@cargo clean


# --- Helper & Default Targets ---

# This is a hidden helper target to ensure `pkg` is set for relevant commands.
.PHONY: check-pkg-defined
check-pkg-defined:
ifndef pkg
	$(error pkg is not set. Please specify a package, e.g., 'make run pkg=node')
endif


.PHONY: help
help:
	@echo "Usage: make <command> [pkg=<package_name>] [LOG_LEVEL=<log_spec>]"
	@echo ""
	@echo "Package-Specific Commands (require 'pkg=<name>'):"
	@echo "  run             - Run a specific package (e.g., 'make run pkg=node')."
	@echo "  build           - Build a specific package for development."
	@echo "  build-release   - Build a specific package for production."
	@echo ""
	@echo "Workspace-Wide Commands:"
	@echo "  test            - Run all tests in the workspace, excluding doc-tests."
	@echo "  doc-test        - Run all doc-tests in the workspace."
	@echo "  check           - Check the entire workspace for errors."
	@echo "  clean           - Remove all build artifacts."
	@echo ""
	@echo "Example:"
	@echo "  make run pkg=node LOG_LEVEL=node=debug,sqlx=warn"
	@echo ""

# Set the default goal to 'help' so that running 'make' provides instructions.
.DEFAULT_GOAL := help

