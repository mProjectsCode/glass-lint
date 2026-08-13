.PHONY: all build check clippy fmt fmt-check test-all test-bundles profile ci clean generate-rules check-rules

CARGO ?= cargo
BUN ?= bun
HARNESS ?= $(CARGO) run -p glass-lint-harness-cli --bin glass-lint-harness --quiet --
HARNESS_SUITE ?= tests/e2e
SAMPLY ?= samply
PROFILE_PATH ?= tests/e2e
PROFILE_PROVIDER ?= obsidian
PROFILE_MODE ?= recommended
PROFILE_ARGS ?= --quiet

all: fmt-check check clippy test-all

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

test-all:
	make test test-e2e test-rules

test-e2e:
	$(HARNESS) verify $(HARNESS_SUITE)

test-bundles:
	@test "$$($(BUN) --version)" = "1.3.14"
	cd tools/bundlers && $(BUN) install --frozen-lockfile && $(BUN) run compatibility-probe.ts
	cd tools/bundlers && $(BUN) run tool-tests.ts
	$(HARNESS) verify tests/e2e/bundles

test-projects:
	$(HARNESS) verify tests/projects

test-rules:
	$(HARNESS) verify glass-lint-js/src/rules
	$(HARNESS) verify glass-lint-obsidian/src/rules

generate-rules:
	$(CARGO) run -p glass-lint-cli --bin glass-lint --quiet -- generate-rules --output RULES.md

check-rules:
	$(CARGO) run -p glass-lint-cli --bin glass-lint --quiet -- generate-rules --check --output RULES.md

profile:
	$(CARGO) build --profile profiling -p glass-lint-harness-cli --bin glass-lint-harness
	$(SAMPLY) record target/profiling/glass-lint-harness profile --path "$(PROFILE_PATH)" --provider "$(PROFILE_PROVIDER)" --profile "$(PROFILE_MODE)" $(PROFILE_ARGS)

compare:
	$(HARNESS) --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts compare $(HARNESS_SUITE)

ci: check clippy test-all check-rules
	$(CARGO) check -p glass-lint-core --examples

clean:
	$(CARGO) clean
