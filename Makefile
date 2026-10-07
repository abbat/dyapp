.PHONY: help install prepare test test-integration test-all coverage coverage-check quality lint check fmt build release security audit deny msrv doc pre-commit pre-push ui-test ui-test-android ui-test-android-emulator ui-test-linux ui-test-shell

DOCKER = python3 scripts/docker-local.py
DEV = $(DOCKER) -f docker/compose.dev.yml --profile test run --no-deps --pull never dev

help:
	@echo "prepare | test | test-integration | test-all | coverage | quality | build | ui-test"

install: prepare

prepare:
	bash scripts/docker-test.sh prepare
	bash scripts/ui-test.sh prepare
	bash scripts/docker-test.sh network-prepare

test:
	bash scripts/docker-test.sh test

test-integration:
	$(DEV) cargo test -p dyapp-bootstrap --test integration_tests --all-features --locked --offline -- --test-threads=1
	bash scripts/docker-test.sh network

test-all:
	bash scripts/docker-test.sh all

coverage coverage-check:
	bash scripts/docker-test.sh coverage

quality lint check:
	bash scripts/check-quality.sh

fmt:
	$(DEV) cargo fmt --all -- --check

build release:
	bash scripts/docker-test.sh build

security audit deny:
	bash scripts/docker-test.sh security

msrv:
	$(DEV) cargo check --workspace --all-features --all-targets --locked --offline

doc:
	$(DEV) cargo doc --workspace --locked --offline --no-deps

pre-commit: quality

pre-push: test-all

ui-test:
	bash scripts/ui-test.sh all

ui-test-android:
	bash scripts/ui-test.sh android

ui-test-android-emulator:
	bash scripts/ui-test.sh android-emulator

ui-test-linux:
	bash scripts/ui-test.sh linux

ui-test-shell:
	bash scripts/ui-test.sh shell android
