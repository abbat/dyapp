# Docker Testing Guide

Run code quality checks and tests in isolated Docker containers.

## Quick Start

### Run Tests

```bash
# Build Docker image for testing
docker build -f docker/Dockerfile.dev -t dyapp:dev .

# Run all unit tests
docker run --rm dyapp:dev cargo test --workspace --lib --quiet

# Run integration tests
docker run --rm dyapp:dev cargo test --test integration_tests

# Run all tests with output
docker run --rm dyapp:dev cargo test --workspace --verbose
```

### Build Release Binary

```bash
# Build optimized release binary
docker run --rm -v $(pwd)/target:/app/target dyapp:dev \
  cargo build --release --workspace

# Binaries will be in ./target/release/
```

### Code Quality Checks

```bash
# Format check
docker run --rm dyapp:dev cargo fmt -- --check

# Lint check
docker run --rm dyapp:dev cargo clippy --workspace --all-features -- -D warnings

# Dependency check
docker run --rm dyapp:dev cargo deny check advisories bans licenses sources
```

## Using Docker Compose

### Available Services

```bash
# Run tests (default)
docker-compose -f docker/compose.dev.yml run --rm --profile test dev

# Build release binary
docker-compose -f docker/compose.dev.yml run --rm --profile build build

# Quality checks
docker-compose -f docker/compose.dev.yml run --rm --profile quality quality

# Code coverage
docker-compose -f docker/compose.dev.yml run --rm --profile coverage coverage

# Security checks
docker-compose -f docker/compose.dev.yml run --rm --profile security security
```

## Using the Helper Script

Prefer `make test`, `make quality` and `make test-all` (or the equivalent helper commands
below). Each captured Docker stage prints one `OK` line, or `FAIL`/`FIX`/`DETAILS`
with selected diagnostics and a retained raw-log pointer.
Set `DYAPP_VERBOSE=1` for live output. Interactive shells stream automatically. See
[local check output](../agents/project.md#local-check-output) for capture limits and
`--log-file`.

```bash
./scripts/docker-test.sh --help
```

### Available Commands

```bash
# Run tests
./scripts/docker-test.sh test

# Build release binary
./scripts/docker-test.sh build

# Quality checks
./scripts/docker-test.sh quality

# Code coverage
./scripts/docker-test.sh coverage

# Security checks
./scripts/docker-test.sh security

# Run all checks
./scripts/docker-test.sh all

# Interactive shell
./scripts/docker-test.sh shell

# Clean up
./scripts/docker-test.sh clean
```

## Examples

### Example 1: Run Tests Only

```bash
$ docker run --rm dyapp:dev cargo test --workspace --lib

running 78 tests
...
test result: ok. 78 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Example 2: Run Specific Crate Tests

```bash
# Test only messaging crate
docker run --rm dyapp:dev cargo test -p dyapp-messaging

# Test only video crate
docker run --rm dyapp:dev cargo test -p dyapp-video
```

### Example 3: Run Tests with Output

```bash
# Show test output (useful for debugging)
docker run --rm dyapp:dev \
  cargo test --workspace --lib -- --nocapture
```

### Example 4: Code Coverage

```bash
# Generate coverage report and apply the line coverage ratchet (cargo-llvm-cov)
bash scripts/docker-test.sh coverage   # or: make coverage
```

## Volume Mounts

Share files between host and container:

```bash
# Mount current directory (read-only)
docker run --rm -v $(pwd):/app:ro dyapp:dev cargo test

# Mount target directory for caching builds
docker run --rm -v $(pwd)/target:/app/target dyapp:dev cargo build --release

# Mount for development (read-write)
docker run --rm -v $(pwd):/app dyapp:dev bash
# Now edit files in /app/src/* and they reflect on host
```

## Environment Variables

```bash
# Set Rust log level
docker run --rm -e RUST_LOG=debug dyapp:dev cargo test

# Set backtrace
docker run --rm -e RUST_BACKTRACE=1 dyapp:dev cargo test

# Disable incremental compilation (faster in containers)
docker run --rm -e CARGO_INCREMENTAL=0 dyapp:dev cargo build
```

## Performance Tips

### Use BuildKit for Faster Builds

```bash
# Enable Docker BuildKit (faster layer caching)
DOCKER_BUILDKIT=1 docker build -f docker/Dockerfile.dev -t dyapp:dev .
```

### Cache Cargo Registry

Docker-compose automatically caches cargo registry in named volumes:

```bash
# Inspect cache
docker volume ls | grep dyapp

# Remove cache
docker volume rm dyapp-dev_cargo-cache
```

### Use `--rm` Flag

Always use `--rm` to automatically remove container after exit:

```bash
# Good - container removed after test
docker run --rm dyapp:dev cargo test

# Bad - leaves container behind
docker run dyapp:dev cargo test
# Remember to clean up: docker ps -a | grep dyapp
```

## Container isolation

`check-container-isolation.sh` requires a non-root runtime, writable `HOME`, zero
effective capabilities, no-new-privileges and no host Docker/X11 sockets. It
prints `OK container-isolation` or a self-contained `FAIL`/`FIX`/`DETAILS` block;
its fix pointer is checked against the central map. `--raw` includes the previous
UID/capability metadata after enforcing the same predicates. Keep the configured
isolation when fixing a failure; do not grant capabilities or mount control
sockets to make the check pass.

## Troubleshooting

Quiet stage failures, shell aggregates, the operator image test and build
preflight failures use `FAIL`/`FIX`/`DETAILS`. Shell aggregates accept `--raw`
to bypass their frontend; help and interactive/verbose modes remain direct.
The image test preserves its original failure over a later temporary-volume
cleanup failure, and a cleanup failure still fails an otherwise successful run.
The raw-log pointer retains unrecognized messages as well as full recognized
errors. Gradle failures select the failed task/dependency block and compiler
errors, omitting passing tasks and generic retry advice. Check Docker daemon
access and the named Compose file/service before retrying. Unsupported build options/configuration fail before image builds so
CPU limits cannot be bypassed. Malformed Compose JSON, missing commands and
startup errors also fail with this contract. Successful configuration JSON is
an internal data value and is not printed as passing progress.

Rust `build`, `test`, `cov-build`, `cov-test`, `cov-report`, `coverage`,
`network` and `memory` stages use the shared runner. Quality/security retain
one result per mapped tool. Make targets `fmt`, `msrv`, `doc` and the explicit
integration-test Cargo command also use mapped runners inside Docker, so a
failed assertion or compiler error keeps its diagnostic context. Config JSON
and coverage JSON/LCOV/text remain data artifacts; interactive shells and
explicit verbose output remain streaming modes. Platform/UI/package/release
entry points are a separate staged rollout, not covered by this inventory.

### Issue: "No such file or directory"

```
error: could not find `Cargo.toml`
```

**Fix:** Run from project root:
```bash
cd dyapp/
docker run --rm dyapp:dev cargo test
```

### Issue: "cargo not found"

```
/bin/bash: cargo: command not found
```

**Fix:** Make sure Docker image was built:
```bash
docker build -f docker/Dockerfile.dev -t dyapp:dev .
```

### Issue: Tests timeout

```
error: could not compile `dyapp`
thread 'main' panicked at 'operation timed out'
```

**Fix:** Increase timeout:
```bash
docker run --rm dyapp:dev cargo test --workspace --release
```

### Issue: Out of disk space

```
no space left on device
```

**Fix:** Clean up Docker:
```bash
docker system prune -a
docker volume prune
```

### Issue: Permission denied on volume mount

```
error: Permission denied
```

**Fix:** Adjust permissions:
```bash
chmod 755 $(pwd)
docker run --rm -v $(pwd):/app dyapp:dev cargo test
```

## Dockerfile Customization

### Add More Tools

Edit `docker/Dockerfile.dev` to install additional tools:

```dockerfile
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Rust tools
RUN cargo install --locked cargo-machete   # example extra tool
```

### Production image (planned)

There is no production image: `rust/bootstrap` has no server binary to package
(see [operator manual](../manuals/bootstrap.md#installing)).
The Dockerfiles in this repository are test images only.

## CI/CD Integration

### GitHub Actions

```yaml
name: Docker Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: docker/setup-buildx-action@v2
      - uses: docker/build-push-action@v4
        with:
          file: ./docker/Dockerfile.dev
          tags: dyapp:ci
          load: true
      - run: docker run --rm dyapp:ci cargo test --workspace
```

### GitLab CI

```yaml
test:
  image: rust:latest
  script:
    - cargo test --workspace --lib --quiet
  cache:
    paths:
      - target/
```

## References

- [Docker Official Rust Image](https://hub.docker.com/_/rust)
- [Docker CLI Reference](https://docs.docker.com/engine/reference/commandline/docker/)
- [Docker Compose Reference](https://docs.docker.com/compose/)
- [BuildKit Documentation](https://docs.docker.com/build/buildkit/)
