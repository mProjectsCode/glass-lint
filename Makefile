.PHONY: all build check clippy fmt fmt-check harness provider-fixtures test ci clean

CARGO ?= cargo
HARNESS ?= $(CARGO) run -p glass-lint-cli --bin glass-lint-harness --
HARNESS_SUITE ?= tests/cases

all: fmt-check check clippy test harness provider-fixtures

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

provider-fixtures:
	$(HARNESS) verify glass-lint-js/src/rules
	$(HARNESS) verify glass-lint-obsidian/src/rules

compare:
	$(HARNESS) --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.mjs compare $(HARNESS_SUITE)

ci: fmt-check check clippy test harness provider-fixtures

clean:
	$(CARGO) clean
