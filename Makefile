.PHONY: all build check clippy fmt fmt-check test test-e2e test-rules ci clean

CARGO ?= cargo
HARNESS ?= $(CARGO) run -p glass-lint-cli --bin glass-lint-harness --
HARNESS_SUITE ?= tests/e2e

all: fmt-check check clippy test test-e2e test-rules

build:
	$(CARGO) build --workspace

check:
	$(CARGO) check --workspace

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

test:
	$(CARGO) test --workspace

test-e2e:
	$(HARNESS) verify $(HARNESS_SUITE)

test-rules:
	$(HARNESS) verify glass-lint-js/src/rules
	$(HARNESS) verify glass-lint-obsidian/src/rules

compare:
	$(HARNESS) --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.mjs compare $(HARNESS_SUITE)

ci: fmt-check check clippy test test-e2e test-rules

clean:
	$(CARGO) clean
