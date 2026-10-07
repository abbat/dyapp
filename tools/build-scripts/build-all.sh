#!/bin/bash
# Build Rust core for all platforms (iOS, Android, Web, Desktop)

set -euo pipefail

YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS_DIR="${PROJECT_ROOT}/tools/build-scripts"

echo -e "${YELLOW}🔨 Building DYApp for all platforms...${NC}"
echo ""

# Parse arguments
BUILD_IOS=false
BUILD_ANDROID=false
BUILD_WEB=false
BUILD_ALL=true

while [[ $# -gt 0 ]]; do
  case $1 in
    --ios)
      BUILD_IOS=true
      BUILD_ALL=false
      shift
      ;;
    --android)
      BUILD_ANDROID=true
      BUILD_ALL=false
      shift
      ;;
    --web)
      BUILD_WEB=true
      BUILD_ALL=false
      shift
      ;;
    *)
      echo "Usage: $0 [--ios] [--android] [--web]"
      echo "  No args: build for all platforms"
      exit 1
      ;;
  esac
done

# Determine what to build
if [ "$BUILD_ALL" = true ]; then
  BUILD_IOS=true
  BUILD_ANDROID=true
  BUILD_WEB=true
fi

# Build iOS
if [ "$BUILD_IOS" = true ]; then
  echo -e "${YELLOW}📱 Building for iOS...${NC}"
  bash "$SCRIPTS_DIR/build-ios.sh"
  echo ""
fi

# Build Android
if [ "$BUILD_ANDROID" = true ]; then
  echo -e "${YELLOW}🤖 Building for Android...${NC}"
  bash "$SCRIPTS_DIR/build-android.sh"
  echo ""
fi

# Build Web (WASM)
if [ "$BUILD_WEB" = true ]; then
  echo -e "${YELLOW}🌐 Building for Web (WASM)...${NC}"
  cd "${PROJECT_ROOT}"
  cargo build --target wasm32-unknown-unknown --release
  echo -e "${GREEN}✓ Web build complete${NC}"
  echo ""
fi

echo -e "${GREEN}✅ All builds complete!${NC}"
