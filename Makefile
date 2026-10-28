.PHONY: help install prepare test test-integration test-all coverage coverage-check mutants fuzz quality lint check lint-android fmt build release deb image security audit deny msrv doc rehearsal pre-commit pre-push ui-test ui-test-android ui-test-android-emulator ui-test-linux ui-test-shell

.SILENT:

DOCKER = python3 tools/scripts/docker-local.py
TEST_COMPOSE = $(DOCKER) -f tools/docker/compose.dev.yml -f tests/docker/compose.dev.yml
DEV = $(TEST_COMPOSE) --profile test run --no-deps --pull never dev
CHECK = $(DEV) python3 -B tools/scripts/run-check.py --log-dir build/check-output

help:
	@echo "prepare | test | test-integration | test-all | coverage | mutants | fuzz | quality | build | deb | image | ui-test"

install: prepare

prepare:
	bash tests/scripts/docker-test.sh prepare
	$(DOCKER) -f tools/docker/compose.dev.yml --profile base build linux-base
	bash tests/scripts/ui-test.sh prepare
	bash tests/scripts/docker-test.sh network-prepare
	$(DOCKER) -f tools/docker/compose.dev.yml --profile deb build deb
	$(TEST_COMPOSE) --profile deb build deb-test

test:
	bash tests/scripts/docker-test.sh test

test-integration:
	$(CHECK) rust-test -- cargo test -p dyapp-bootstrap --test integration_tests --all-features --locked --offline
	bash tests/scripts/docker-test.sh network
	$(DEV) bash tests/scripts/rust-check.sh memory

# First-deployment rehearsal with production defaults, upgrading from FROM (a git ref); ~3 h.
rehearsal:
	$(DEV) env REHEARSAL_FROM=$(FROM) bash tests/scripts/rust-check.sh rehearsal

test-all:
	bash tests/scripts/docker-test.sh all

coverage coverage-check:
	bash tests/scripts/docker-test.sh coverage

# Practice B2: every mutant in Rust code changed since origin/main is caught.
mutants:
	$(DEV) bash tests/scripts/rust-check.sh mutants

# Practice B4: every fuzz target for FUZZ_SECONDS (default 60).
fuzz:
	$(DEV) env FUZZ_SECONDS=$${FUZZ_SECONDS:-60} bash tests/scripts/rust-check.sh fuzz

quality lint check:
	bash tests/scripts/check-quality.sh

fmt:
	$(CHECK) rustfmt -- cargo fmt --all -- --check

build release:
	python3 tools/scripts/run-check.py --log-dir build/check-output rust-build -- $(DOCKER) -f tools/docker/compose.dev.yml --profile build run --no-deps --pull never build

# build/deb/dyappd_<version>_<arch>.deb and dyappd-<version>-<machine>-linux-gnu.tar.gz, then a
# test install of each on a clean Debian 12; the same pair for the desktop client (dyapp_...,
# dyapp-...), installed with apt and smoke-tested under Xvfb.
deb:
	mkdir -p build/deb
	DYAPP_UID=$$(id -u) DYAPP_GID=$$(id -g) $(DOCKER) -f tools/docker/compose.dev.yml --profile deb run --rm --no-deps --pull never deb
	$(TEST_COMPOSE) --profile deb run --rm --no-deps --pull never deb-test
	$(TEST_COMPOSE) --profile deb run --rm --no-deps --pull never tarball-test
	$(TEST_COMPOSE) --profile deb run --rm --no-deps --pull never client-deb-test

# dyappd:latest, then a run with a read-only root, no capabilities and one volume.
image:
	$(DOCKER) -f tools/docker/compose.dev.yml --profile image build dyappd
	bash tests/scripts/image-test.sh

security audit deny:
	bash tests/scripts/docker-test.sh security

msrv:
	$(CHECK) rust-build -- cargo check --workspace --all-features --all-targets --locked --offline

doc:
	$(CHECK) rustdoc -- cargo doc --workspace --locked --offline --no-deps

lint-android:
	bash tests/scripts/docker-test.sh lint-android

pre-commit: quality

pre-push: test-all

ui-test:
	bash tests/scripts/ui-test.sh all

ui-test-android:
	bash tests/scripts/ui-test.sh android

ui-test-android-emulator:
	bash tests/scripts/ui-test.sh android-emulator

ui-test-linux:
	bash tests/scripts/ui-test.sh linux

ui-test-shell:
	bash tests/scripts/ui-test.sh shell android
