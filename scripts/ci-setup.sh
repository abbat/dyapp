#!/bin/bash
# Prepares a GitHub-hosted Ubuntu runner to run the checks without Docker. CI only: it installs
# system packages with sudo. The package lists mirror docker/Dockerfile.dev and Dockerfile.linux-test.
set -euo pipefail
toolchain=1.99.0
case "${1:?usage: ci-setup.sh rust|linux}" in
    rust)
        packages=(build-essential pkg-config protobuf-compiler libssl-dev
                  python3-flake8 python3-yaml python3-pyflakes shellcheck)
        components=(--component rustfmt,clippy,llvm-tools-preview)
        ;;
    linux)
        packages=(build-essential pkg-config libssl-dev libgtk-3-dev libwebkit2gtk-4.0-dev
                  libayatana-appindicator3-dev librsvg2-dev xvfb xauth dbus-x11)
        components=()
        ;;
    *) echo "Unknown target: $1" >&2; exit 2 ;;
esac
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends "${packages[@]}"
if [[ $1 == rust ]]; then
    # Official actionlint 1.7.12, the same image digest as docker/Dockerfile.dev.
    container=$(docker create rhysd/actionlint@sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667)
    sudo docker cp "$container:/usr/local/bin/actionlint" /usr/local/bin/actionlint
    docker rm "$container" > /dev/null
fi
rustup toolchain install "$toolchain" --profile minimal "${components[@]}"
echo "RUSTUP_TOOLCHAIN=$toolchain" >> "${GITHUB_ENV:?GitHub Actions only}"
