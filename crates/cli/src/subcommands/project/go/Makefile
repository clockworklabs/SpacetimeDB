# Makefile for SpacetimeDB Go Module

# Variables
MODULE_NAME := spacetime-module
WASM_FILE := $(MODULE_NAME).wasm
BUILD_DIR := build

# Go build settings for WASM
export GOOS := wasip1
export GOARCH := wasm

.PHONY: all build clean test fmt lint deps help

# Default target
all: build

# Build the WASM module
build:
	@echo "Building WASM module..."
	@mkdir -p $(BUILD_DIR)
	go build -o $(BUILD_DIR)/$(WASM_FILE) .
	@echo "Built $(BUILD_DIR)/$(WASM_FILE)"

# Generate client bindings
generate: build
	@echo "Generating client bindings..."
	spacetime generate --lang=typescript --out-dir=./bindings/typescript --bin-path=$(BUILD_DIR)/$(WASM_FILE)
	spacetime generate --lang=csharp --out-dir=./bindings/csharp --bin-path=$(BUILD_DIR)/$(WASM_FILE)
	spacetime generate --lang=rust --out-dir=./bindings/rust --bin-path=$(BUILD_DIR)/$(WASM_FILE)
	@echo "Generated client bindings"

# Deploy to SpacetimeDB
deploy: build
	@echo "Deploying to SpacetimeDB..."
	spacetime publish $(BUILD_DIR)/$(WASM_FILE) --project-type wasm
	@echo "Deployed successfully"

# Run tests
test:
	@echo "Running tests..."
	go test ./...

# Format code
fmt:
	@echo "Formatting code..."
	go fmt ./...

# Lint code
lint:
	@echo "Linting code..."
	go vet ./...

# Install/update dependencies
deps:
	@echo "Installing dependencies..."
	go mod tidy
	go mod download

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	rm -rf $(BUILD_DIR)
	rm -rf bindings/
	go clean

# Development workflow
dev: fmt lint test build
	@echo "Development build complete"

# Help
help:
	@echo "Available targets:"
	@echo "  build     - Build the WASM module"
	@echo "  generate  - Generate client bindings for all languages"
	@echo "  deploy    - Deploy the module to SpacetimeDB"
	@echo "  test      - Run tests"
	@echo "  fmt       - Format code"
	@echo "  lint      - Lint code"
	@echo "  deps      - Install/update dependencies"
	@echo "  clean     - Clean build artifacts"
	@echo "  dev       - Run format, lint, test, and build"
	@echo "  help      - Show this help message" 