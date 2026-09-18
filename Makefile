.PHONY: build release run test fmt fmt-check lint clean install check all

BIN := csvsum
SAMPLE := examples/sample.csv

all: build

## Build a debug binary
build:
	cargo build

## Build an optimized release binary
release:
	cargo build --release

## Run against the bundled sample CSV (override with ARGS="...")
run: build
	cargo run -- $(if $(ARGS),$(ARGS),$(SAMPLE))

## Run the test suite
test:
	cargo test

## Format the code
fmt:
	cargo fmt

## Check formatting without changing files
fmt-check:
	cargo fmt --check

## Run clippy with warnings denied
lint:
	cargo clippy --all-targets -- -D warnings

## fmt-check + lint + test, useful before committing
check: fmt-check lint test

## Install the release binary to ~/.cargo/bin
install: release
	cargo install --path .

## Remove build artifacts
clean:
	cargo clean
