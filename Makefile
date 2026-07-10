.PHONY: all build check clippy fmt fmt-check harness test ci clean

CARGO ?= cargo
HARNESS ?= $(CARGO) run -p glass-lint-cli --bin glass-lint-harness --
HARNESS_SUITE ?= tests/cases

all: fmt-check check clippy test harness

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

harness:
	$(HARNESS) verify $(HARNESS_SUITE)

compare:
	$(HARNESS) --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.mjs compare $(HARNESS_SUITE)

ci: fmt-check check clippy test harness

clean:
	$(CARGO) clean
