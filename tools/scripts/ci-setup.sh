#!/bin/bash
# Prepares a GitHub-hosted Ubuntu runner to run the checks without Docker. CI only: it installs
# system packages with sudo. The package lists mirror tools/docker/Dockerfile.dev and Dockerfile.linux-test.
set -euo pipefail
toolchain=1.99.0
case "${1:?usage: ci-setup.sh rust|linux}" in
    rust)
        packages=(build-essential pkg-config protobuf-compiler libssl-dev
                  python3-flake8 python3-yaml python3-pyflakes shellcheck)
        components=(--component "rustfmt,clippy,llvm-tools-preview")
        ;;
    linux)
        packages=(build-essential pkg-config libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev
                  libayatana-appindicator3-dev librsvg2-dev xvfb xauth dbus-x11)
        components=()
        ;;
    *) echo "Unknown target: $1" >&2; exit 2 ;;
esac
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends "${packages[@]}"
if [[ $1 == rust ]]; then
    # Official actionlint 1.7.12, the same image digest as tools/docker/Dockerfile.dev.
    container=$(docker create rhysd/actionlint@sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667)
    sudo docker cp "$container:/usr/local/bin/actionlint" /usr/local/bin/actionlint
    docker rm "$container" > /dev/null
    # Official gitleaks 8.30.1, the same image digest as tools/docker/Dockerfile.dev.
    container=$(docker create zricethezav/gitleaks@sha256:c00b6bd0aeb3071cbcb79009cb16a60dd9e0a7c60e2be9ab65d25e6bc8abbb7f)
    sudo docker cp "$container:/usr/bin/gitleaks" /usr/local/bin/gitleaks
    docker rm "$container" > /dev/null
    # typos 1.50.3, taplo 0.10.0, cargo-shear 1.14.0, SwiftLint 0.65.1, detekt 1.23.8 and zizmor 1.30.1 release
    # binaries, the same checksums as tools/docker/Dockerfile.dev. detekt runs on the runner's Java.
    tmp=$(mktemp -d)
    curl -fsSL -o "$tmp/typos.tgz" https://github.com/crate-ci/typos/releases/download/v1.50.3/typos-v1.50.3-x86_64-unknown-linux-musl.tar.gz
    curl -fsSL -o "$tmp/taplo.gz" https://github.com/tamasfe/taplo/releases/download/0.10.0/taplo-linux-x86_64.gz
    curl -fsSL -o "$tmp/shear.tgz" https://github.com/Boshen/cargo-shear/releases/download/v1.14.0/cargo-shear-x86_64-unknown-linux-musl.tar.gz
    curl -fsSL -o "$tmp/swiftlint.zip" https://github.com/realm/SwiftLint/releases/download/0.65.1/swiftlint_linux_amd64.zip
    curl -fsSL -o "$tmp/zizmor.tgz" https://github.com/zizmorcore/zizmor/releases/download/v1.30.1/zizmor-x86_64-unknown-linux-gnu.tar.gz
    curl -fsSL -o "$tmp/detekt-cli.jar" https://github.com/detekt/detekt/releases/download/v1.23.8/detekt-cli-1.23.8-all.jar
    (cd "$tmp" && printf '%s\n' 'aca6b5d546307092b8d0a8e0a89dd80f9da51f2f7617c5e45c5607c1684ffbf2  typos.tgz' \
        '8fe196b894ccf9072f98d4e1013a180306e17d244830b03986ee5e8eabeb6156  taplo.gz' \
        '9004df4eb031a456026c4326ae0d0ce545321527a67be90f4524c1be729dc40e  shear.tgz' \
        'caeed6f4a679c35539ffaf124f6c4ab4a8416917f7d8796279dc52b74026059d  swiftlint.zip' \
        'e65324f4430c2717591937edcec90ccbefaf14c174f8ec9415e03ca875b46e1a  zizmor.tgz' \
        '2ce2ff952e150baf28a29cda70a363b0340b3e81a55f43e51ec5edffc3d066c1  detekt-cli.jar' | sha256sum -c -)
    tar -xzf "$tmp/typos.tgz" -C "$tmp" ./typos
    tar -xzf "$tmp/shear.tgz" -C "$tmp" cargo-shear
    tar -xzf "$tmp/zizmor.tgz" -C "$tmp" zizmor
    gunzip "$tmp/taplo.gz"
    unzip -p "$tmp/swiftlint.zip" swiftlint-static > "$tmp/swiftlint"
    sudo install -m 755 "$tmp/typos" "$tmp/taplo" "$tmp/cargo-shear" "$tmp/swiftlint" \
        "$tmp/zizmor" /usr/local/bin/
    sudo install -m 644 "$tmp/detekt-cli.jar" /usr/local/lib/
    rm -rf "$tmp"
fi
rustup toolchain install "$toolchain" --profile minimal "${components[@]}"
echo "RUSTUP_TOOLCHAIN=$toolchain" >> "${GITHUB_ENV:?GitHub Actions only}"
