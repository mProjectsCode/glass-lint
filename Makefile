.PHONY: all build check clippy fmt fmt-check test test-e2e test-rules profile ci clean

CARGO ?= cargo
HARNESS ?= $(CARGO) run -p glass-lint-cli --bin glass-lint-harness --
HARNESS_SUITE ?= tests/e2e
SAMPLY ?= samply
PROFILE_PATH ?= tests/e2e
PROFILE_PROVIDER ?= obsidian
PROFILE_MODE ?= recommended
PROFILE_ARGS ?= --quiet

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

profile:
	$(CARGO) build --profiling -p glass-lint-cli --bin glass-lint-harness
	$(SAMPLY) record target/profiling/glass-lint-harness profile --path "$(PROFILE_PATH)" --provider "$(PROFILE_PROVIDER)" --profile "$(PROFILE_MODE)" $(PROFILE_ARGS)

compare:
	$(HARNESS) --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts compare $(HARNESS_SUITE)

ci: fmt-check check clippy test test-e2e test-rules

clean:
	$(CARGO) clean
