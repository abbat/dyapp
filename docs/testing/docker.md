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
docker run --rm dyapp:dev cargo deny check advisories duplicates bans licenses
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
  cargo test --workspace --lib -- --nocapture --test-threads=1
```

### Example 4: Code Coverage

```bash
# Generate coverage report and apply the 70% line gate (cargo-llvm-cov)
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

## Troubleshooting

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
(see [Deployment Guide](../operations/deployment.md#bootstrapping-a-node)).
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
      - uses: actions/checkout@v4
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
