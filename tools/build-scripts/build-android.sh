#!/bin/bash
# Build Rust core for Android targets
# Generates native libraries (.so) for Gradle integration

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/target/android-build"
ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-}"
RUST_CRATES=("dyapp-identity" "dyapp-profile" "dyapp-p2p-net" "dyapp-messaging" "dyapp-video")

# Android targets (ABI mapping)
declare -A ANDROID_TARGETS=(
  ["aarch64-linux-android"]="arm64-v8a"
  ["armv7-linux-androideabi"]="armeabi-v7a"
)

echo -e "${YELLOW}🤖 Building Rust core for Android...${NC}"

# Check prerequisites
check_prerequisites() {
  if ! command -v rustup &> /dev/null; then
    echo -e "${RED}❌ Rust not found. Install from https://rustup.rs/${NC}"
    exit 1
  fi

  if ! command -v cargo-ndk &> /dev/null; then
    echo -e "${YELLOW}⚠️  cargo-ndk not found. Installing...${NC}"
    cargo install cargo-ndk
  fi

  if [ -z "$ANDROID_NDK_HOME" ]; then
    echo -e "${RED}❌ ANDROID_NDK_HOME not set${NC}"
    echo "   Set via: export ANDROID_NDK_HOME=/path/to/android-ndk"
    exit 1
  fi

  echo -e "${GREEN}✓ Prerequisites OK${NC}"
}

# Install Android targets
install_targets() {
  echo -e "${YELLOW}📦 Installing Android targets...${NC}"
  for target in "${!ANDROID_TARGETS[@]}"; do
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

  # Use cargo-ndk for NDK integration
  cargo ndk \
    -t "$target" \
    -o "${BUILD_DIR}/lib" \
    -- build -p dyapp-identity \
    -p dyapp-profile \
    -p dyapp-p2p-net \
    -p dyapp-messaging \
    -p dyapp-video \
    --release --lib

  echo -e "${GREEN}✓ Built for $target${NC}"
}

# Organize libraries for Gradle
organize_libraries() {
  echo -e "${YELLOW}📦 Organizing libraries for Gradle...${NC}"

  mkdir -p "${BUILD_DIR}/gradle"

  # Create jniLibs structure for Android
  for target in "${!ANDROID_TARGETS[@]}"; do
    local abi="${ANDROID_TARGETS[$target]}"
    local lib_dir="${BUILD_DIR}/gradle/jniLibs/${abi}"
    mkdir -p "$lib_dir"

    # Copy .so files
    if [ -d "${BUILD_DIR}/lib/${target}" ]; then
      cp "${BUILD_DIR}/lib/${target}"/*.so "$lib_dir/" 2>/dev/null || true
    fi
  done

  echo -e "${GREEN}✓ Libraries organized${NC}"
}

# Generate Gradle integration code
generate_gradle_integration() {
  echo -e "${YELLOW}🔧 Generating Gradle integration...${NC}"

  local build_gradle_path="${PROJECT_ROOT}/android/build-rust.gradle"

  cat > "$build_gradle_path" <<'EOF'
// Gradle build configuration for Rust FFI libraries
// Include this file in your android/app/build.gradle:
// apply from: '../build-rust.gradle'

task buildRustLibraries {
    doLast {
        exec {
            executable = 'bash'
            args = ['tools/build-scripts/build-android.sh']
        }
    }
}

preBuild.dependsOn buildRustLibraries

// Copy built libraries to jniLibs
afterEvaluate {
    preBuild.doLast {
        def rustBuildDir = file('target/android-build/gradle/jniLibs')
        if (rustBuildDir.exists()) {
            def jniDir = file("${android.sourceSets.main.jniLibs.srcDirs[0]}")
            copy {
                from rustBuildDir
                into jniDir
            }
        }
    }
}
EOF

  echo -e "${GREEN}✓ Gradle integration generated at: $build_gradle_path${NC}"
}

# Generate CMakeLists.txt for NDK
generate_cmake() {
  echo -e "${YELLOW}📝 Generating CMakeLists.txt...${NC}"

  local cmake_path="${PROJECT_ROOT}/android/CMakeLists.txt"

  cat > "$cmake_path" <<'EOF'
cmake_minimum_required(VERSION 3.6)
project(dyapp_jni)

# Add Rust-built libraries
add_library(dyapp_core SHARED IMPORTED)

# Target architecture
set(RUST_TARGET_DIR ${CMAKE_SOURCE_DIR}/target/android-build/gradle/jniLibs/${ANDROID_ABI})

set_target_properties(dyapp_core PROPERTIES
    IMPORTED_LOCATION ${RUST_TARGET_DIR}/libdyapp_core.so
)

include_directories(${CMAKE_SOURCE_DIR}/include)
EOF

  echo -e "${GREEN}✓ CMakeLists.txt generated${NC}"
}

# Verify build output
verify_build() {
  echo -e "${YELLOW}🔍 Verifying build output...${NC}"

  local found=false
  for target in "${!ANDROID_TARGETS[@]}"; do
    local abi="${ANDROID_TARGETS[$target]}"
    local lib_dir="${BUILD_DIR}/gradle/jniLibs/${abi}"
    if [ -d "$lib_dir" ] && ls "$lib_dir"/*.so &> /dev/null; then
      echo "  ✓ $abi: $(ls -1 "$lib_dir"/*.so | wc -l) libraries"
      found=true
    fi
  done

  if [ "$found" = false ]; then
    echo -e "${RED}❌ No libraries found in build output${NC}"
    exit 1
  fi

  echo -e "${GREEN}✓ Build verified${NC}"
}

# Main build flow
main() {
  echo "Project root: $PROJECT_ROOT"
  echo "Build directory: $BUILD_DIR"
  echo "NDK home: $ANDROID_NDK_HOME"
  echo ""

  check_prerequisites
  install_targets

  # Build for each target
  for target in "${!ANDROID_TARGETS[@]}"; do
    build_target "$target"
  done

  organize_libraries
  generate_gradle_integration
  generate_cmake
  verify_build

  echo ""
  echo -e "${GREEN}✅ Android build complete!${NC}"
  echo "Output: $BUILD_DIR/gradle/jniLibs"
  echo ""
  echo "To integrate with Gradle:"
  echo "  1. Add to android/app/build.gradle:"
  echo "     apply from: '../build-rust.gradle'"
  echo "  2. Build: ./gradlew build"
}

main "$@"
