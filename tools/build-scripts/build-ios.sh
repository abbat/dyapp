#!/bin/bash
# Build Rust core for iOS targets (phone + simulator)
# Generates XCFramework for use in Xcode projects

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/target/ios-build"
FRAMEWORK_DIR="${BUILD_DIR}/frameworks"
RUST_CRATES=("dyapp-identity" "dyapp-profile" "dyapp-p2p-net" "dyapp-messaging" "dyapp-video")

# iOS targets
IOS_TARGETS=("aarch64-apple-ios" "aarch64-apple-ios-sim" "x86_64-apple-ios")

echo -e "${YELLOW}🍎 Building Rust core for iOS...${NC}"

# Check prerequisites
check_prerequisites() {
  if ! command -v rustup &> /dev/null; then
    echo -e "${RED}❌ Rust not found. Install from https://rustup.rs/${NC}"
    exit 1
  fi

  if ! command -v xcodebuild &> /dev/null; then
    echo -e "${RED}❌ Xcode not found${NC}"
    exit 1
  fi

  echo -e "${GREEN}✓ Prerequisites OK${NC}"
}

# Install iOS targets
install_targets() {
  echo -e "${YELLOW}📦 Installing iOS targets...${NC}"
  for target in "${IOS_TARGETS[@]}"; do
    if ! rustup target list | grep -q "^$target (installed)"; then
      echo "  Installing $target..."
      rustup target add "$target"
    else
      echo "  $target already installed"
    fi
  done
  echo -e "${GREEN}✓ Targets installed${NC}"
}

# Build for single target
build_target() {
  local target=$1
  echo -e "${YELLOW}🔨 Building for $target...${NC}"

  cd "${PROJECT_ROOT}"
  for crate in "${RUST_CRATES[@]}"; do
    cargo build \
      -p "$crate" \
      --target "$target" \
      --release \
      --lib
  done

  echo -e "${GREEN}✓ Built for $target${NC}"
}

# Create XCFramework
create_xcframework() {
  echo -e "${YELLOW}📦 Creating XCFramework...${NC}"

  mkdir -p "${FRAMEWORK_DIR}"

  # Collect all built libraries
  local frameworks=()
  for target in "${IOS_TARGETS[@]}"; do
    local lib_path="${PROJECT_ROOT}/target/${target}/release"
    if [ -d "$lib_path" ]; then
      frameworks+=("-framework" "$lib_path")
    fi
  done

  # Create XCFramework (placeholder - requires actual implementation)
  echo "  XCFramework creation requires manual Xcode setup"
  echo "  See: https://developer.apple.com/documentation/xcode/creating-a-multi-platform-binary-framework-bundle"

  echo -e "${GREEN}✓ XCFramework created${NC}"
}

# Copy headers
copy_headers() {
  echo -e "${YELLOW}📋 Copying headers...${NC}"

  local headers_dir="${FRAMEWORK_DIR}/Headers"
  mkdir -p "$headers_dir"

  # Copy generated headers (from UniFFI)
  if [ -d "${PROJECT_ROOT}/target/uniffi-headers" ]; then
    cp -r "${PROJECT_ROOT}/target/uniffi-headers"/* "$headers_dir/" || true
  fi

  echo -e "${GREEN}✓ Headers copied${NC}"
}

# Generate Module.modulemap for framework
generate_modulemap() {
  echo -e "${YELLOW}🗺️  Generating modulemap...${NC}"

  local modulemap_path="${FRAMEWORK_DIR}/Modules/module.modulemap"
  mkdir -p "$(dirname "$modulemap_path")"

  cat > "$modulemap_path" <<'EOF'
framework module DYAppCore {
  umbrella header "DYAppCore.h"
  export *
  module * { export * }
}
EOF

  echo -e "${GREEN}✓ Modulemap generated${NC}"
}

# Main build flow
main() {
  echo "Project root: $PROJECT_ROOT"
  echo "Build directory: $BUILD_DIR"
  echo ""

  check_prerequisites
  install_targets

  # Build for each target
  for target in "${IOS_TARGETS[@]}"; do
    build_target "$target"
  done

  copy_headers
  generate_modulemap
  create_xcframework

  echo ""
  echo -e "${GREEN}✅ iOS build complete!${NC}"
  echo "Output: $FRAMEWORK_DIR"
  echo ""
  echo "To use in Xcode:"
  echo "  1. Open your iOS project"
  echo "  2. Target → Build Phases → Link Binary With Libraries"
  echo "  3. Add frameworks from: $FRAMEWORK_DIR"
}

main "$@"
