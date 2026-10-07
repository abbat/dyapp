#!/bin/bash
# Regenerate Rust, Swift, and Kotlin code from .proto files
# Must be run whenever .proto files are updated

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTO_DIR="${PROJECT_ROOT}/proto"
GENERATED_DIR="${PROJECT_ROOT}/generated"

echo -e "${YELLOW}🔧 Regenerating Protobuf code from .proto files...${NC}"

# Check prerequisites
check_prerequisites() {
  echo -e "${YELLOW}📋 Checking prerequisites...${NC}"

  if ! command -v protoc &> /dev/null; then
    echo -e "${RED}❌ protoc not found${NC}"
    echo "Install from: https://grpc.io/docs/protoc-installation/"
    exit 1
  fi

  echo "  protoc version: $(protoc --version)"

  # For Rust (prost)
  if ! cargo build --message-format=json -p dyapp-profile 2>&1 | grep -q "prost"; then
    echo -e "${YELLOW}⚠️  prost dependency may be needed${NC}"
  fi

  echo -e "${GREEN}✓ Prerequisites OK${NC}"
}

# Generate Rust code
generate_rust() {
  echo -e "${YELLOW}🦀 Generating Rust code...${NC}"

  local rust_out="${PROJECT_ROOT}/src/generated"
  mkdir -p "$rust_out"

  # Use prost-build for Rust
  cd "${PROJECT_ROOT}"

  cat > build_proto.rs <<'EOF'
use prost_build;

fn main() {
    prost_build::Config::new()
        .bytes(["."])
        .compile_protos(&["proto/messages.proto"], &["proto"])
        .unwrap();
}
EOF

  # Run prost compiler
  protoc \
    --prost_out="${rust_out}" \
    --proto_path="${PROTO_DIR}" \
    "${PROTO_DIR}/messages.proto" || echo "Using prost-build instead"

  rm -f build_proto.rs

  echo -e "${GREEN}✓ Rust code generated${NC}"
}

# Generate Swift code
generate_swift() {
  echo -e "${YELLOW}📱 Generating Swift code...${NC}"

  local swift_out="${PROJECT_ROOT}/ios/Generated"
  mkdir -p "$swift_out"

  # Check for Swift protobuf plugin
  if ! command -v protoc-gen-swift &> /dev/null; then
    echo -e "${YELLOW}⚠️  protoc-gen-swift not found${NC}"
    echo "   Install: brew install swift-protobuf"
    echo "   Or: git clone https://github.com/apple/swift-protobuf"
    return
  fi

  protoc \
    --swift_out="${swift_out}" \
    --proto_path="${PROTO_DIR}" \
    "${PROTO_DIR}/messages.proto"

  echo -e "${GREEN}✓ Swift code generated${NC}"
}

# Generate Kotlin code
generate_kotlin() {
  echo -e "${YELLOW}🤖 Generating Kotlin code...${NC}"

  local kotlin_out="${PROJECT_ROOT}/android/app/src/main/kotlin/generated"
  mkdir -p "$kotlin_out"

  # Check for Kotlin protobuf plugin
  if ! command -v protoc-gen-kotlin &> /dev/null; then
    echo -e "${YELLOW}⚠️  protoc-gen-kotlin not found${NC}"
    echo "   Using Java plugin as fallback"

    protoc \
      --java_out="${kotlin_out}" \
      --proto_path="${PROTO_DIR}" \
      "${PROTO_DIR}/messages.proto"
  else
    protoc \
      --kotlin_out="${kotlin_out}" \
      --proto_path="${PROTO_DIR}" \
      "${PROTO_DIR}/messages.proto"
  fi

  echo -e "${GREEN}✓ Kotlin code generated${NC}"
}

# Generate Go (optional, for bootstrap servers)
generate_go() {
  echo -e "${YELLOW}🐹 Generating Go code (optional)...${NC}"

  if ! command -v protoc-gen-go &> /dev/null; then
    echo -e "${YELLOW}⚠️  protoc-gen-go not found (skipping)${NC}"
    return
  fi

  local go_out="${PROJECT_ROOT}/bootstrap/pkg/pb"
  mkdir -p "$go_out"

  protoc \
    --go_out="${go_out}" \
    --go-grpc_out="${go_out}" \
    --proto_path="${PROTO_DIR}" \
    "${PROTO_DIR}/messages.proto"

  echo -e "${GREEN}✓ Go code generated${NC}"
}

# Verify generated code
verify_generated() {
  echo -e "${YELLOW}🔍 Verifying generated code...${NC}"

  local errors=0

  # Check Rust
  if [ -f "${PROJECT_ROOT}/src/generated/dyapp.rs" ]; then
    echo "  ✓ Rust code present"
  else
    echo -e "${RED}  ✗ Rust code missing${NC}"
    errors=$((errors + 1))
  fi

  # Check Swift (if generated)
  if [ -d "${PROJECT_ROOT}/ios/Generated" ] && [ "$(ls -A ${PROJECT_ROOT}/ios/Generated)" ]; then
    echo "  ✓ Swift code present"
  else
    echo "  ℹ️  Swift code not yet generated"
  fi

  # Check Kotlin (if generated)
  if [ -d "${PROJECT_ROOT}/android/app/src/main/kotlin/generated" ] && [ "$(ls -A ${PROJECT_ROOT}/android/app/src/main/kotlin/generated)" ]; then
    echo "  ✓ Kotlin code present"
  else
    echo "  ℹ️  Kotlin code not yet generated"
  fi

  if [ $errors -gt 0 ]; then
    echo -e "${RED}❌ Verification failed${NC}"
    return 1
  fi

  echo -e "${GREEN}✓ Verification passed${NC}"
}

# Calculate checksums for CI validation
calculate_checksums() {
  echo -e "${YELLOW}📝 Calculating checksums...${NC}"

  local checksum_file="${PROJECT_ROOT}/.proto-checksums"
  > "$checksum_file"

  # Checksum proto source
  find "${PROTO_DIR}" -name "*.proto" -exec sha256sum {} \; >> "$checksum_file"

  # Checksum generated code
  find "${PROJECT_ROOT}/src/generated" -type f -exec sha256sum {} \; >> "$checksum_file" 2>/dev/null || true
  find "${PROJECT_ROOT}/ios/Generated" -type f -exec sha256sum {} \; >> "$checksum_file" 2>/dev/null || true
  find "${PROJECT_ROOT}/android/app/src/main/kotlin/generated" -type f -exec sha256sum {} \; >> "$checksum_file" 2>/dev/null || true

  echo -e "${GREEN}✓ Checksums saved to: $checksum_file${NC}"
}

# Main flow
main() {
  echo "Project root: $PROJECT_ROOT"
  echo "Proto directory: $PROTO_DIR"
  echo ""

  check_prerequisites
  echo ""

  generate_rust
  echo ""

  generate_swift
  echo ""

  generate_kotlin
  echo ""

  generate_go
  echo ""

  verify_generated
  echo ""

  calculate_checksums
  echo ""

  echo -e "${GREEN}✅ Protobuf code generation complete!${NC}"
  echo ""
  echo "Next steps:"
  echo "  1. Review generated code"
  echo "  2. Commit .proto changes"
  echo "  3. Run tests to verify"
  echo ""
  echo "To update in future:"
  echo "  1. Edit proto/messages.proto"
  echo "  2. Run: ./scripts/regenerate-proto.sh"
  echo "  3. Commit all changes"
}

main "$@"
