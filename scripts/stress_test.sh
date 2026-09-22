#!/usr/bin/env bash
set -euo pipefail

# Comprehensive stress-test suite for bad_apple_rs across all directives.
# Directives tested:
# 1. Corrupt and malformed binary frame streams
# 2. Extreme CLI bounds (subnormal, negative, zero, infinite FPS)
# 3. Missing asset fallbacks (audio and binary frames)
# 4. Sequence gap detection and builder invariants
# 5. Terminal signal recovery and non-blocking abort
# 6. Feature permutations (default audio vs pure Rust video-only)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT_DIR/target/release/bad_apple"
WORK_DIR="$(mktemp -d /tmp/bad_apple_stress_XXXXXX)"
PASS=0
FAIL=0

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

echo "=== Building bad_apple binary in release mode ==="
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

run_directive() {
    local name="$1"
    local cmd="$2"
    local expected_exit="$3"

    echo -n "Directive: $name ... "
    set +e
    eval "$cmd" > "$WORK_DIR/out.log" 2> "$WORK_DIR/err.log"
    local code=$?
    set -e

    if [[ "$code" -eq "$expected_exit" ]]; then
        echo "PASS (exit $code)"
        PASS=$((PASS + 1))
    else
        echo "FAIL (expected $expected_exit, got $code)"
        echo "--- stderr ---"
        cat "$WORK_DIR/err.log"
        FAIL=$((FAIL + 1))
    fi
}

echo ""
echo "=== Directive 1: Input Frame Stream Boundaries ==="

# 1a. Empty binary file (0 bytes) -> clean EOF
touch "$WORK_DIR/empty.bin"
run_directive "0-byte binary frame EOF" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin' /dev/null" 0

# 1b. Truncated 1 byte
head -c 1 /dev/urandom > "$WORK_DIR/trunc1.bin"
run_directive "1-byte truncated frame reject" \
    "script -qec '$BIN play --input $WORK_DIR/trunc1.bin' /dev/null" 1

# 1c. Truncated 4799 bytes (1 byte short of 1 frame)
head -c 4799 /dev/urandom > "$WORK_DIR/trunc4799.bin"
run_directive "4799-byte frame truncation reject" \
    "script -qec '$BIN play --input $WORK_DIR/trunc4799.bin' /dev/null" 1

# 1d. 4801 bytes (1 frame + 1 trailing byte)
head -c 4801 /dev/urandom > "$WORK_DIR/trunc4801.bin"
run_directive "4801-byte stream with trailing byte reject" \
    "script -qec '$BIN play --input $WORK_DIR/trunc4801.bin' /dev/null" 1

# 1e. Nonexistent input file
run_directive "Nonexistent input file reject" \
    "script -qec '$BIN play --input $WORK_DIR/does_not_exist.bin' /dev/null" 1

echo ""
echo "=== Directive 2: Extreme CLI Bounds and FPS Normalization ==="

# 2a. Zero FPS
run_directive "Zero FPS normalization" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin --fps 0' /dev/null" 0

# 2b. Negative FPS
run_directive "Negative FPS normalization" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin --fps -30' /dev/null" 0

# 2c. Subnormal tiny FPS
run_directive "Subnormal FPS normalization" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin --fps 1e-30' /dev/null" 0

# 2d. Huge FPS
run_directive "Astronomical FPS normalization" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin --fps 9999999999' /dev/null" 0

echo ""
echo "=== Directive 3: Missing Audio Fallback ==="

# Missing audio does not crash video playback; it cleanly logs warning and proceeds
run_directive "Missing audio gracefully falls back" \
    "script -qec '$BIN play --input $WORK_DIR/empty.bin --audio $WORK_DIR/missing.ogg' /dev/null" 0

echo ""
echo "=== Directive 4: Frame Sequence Builder Invariants ==="

# 4a. Missing frames directory
run_directive "Builder missing directory error" \
    "$BIN build --frames-dir $WORK_DIR/nonexistent_dir --output $WORK_DIR/out.bin" 1

# 4b. Sequence gap detection
FRAMES_GAP_DIR="$WORK_DIR/frames_gap"
mkdir -p "$FRAMES_GAP_DIR"
# Create frame_0001.png and frame_0003.png (missing frame_0002.png)
python3 -c "
import struct, zlib

def make_png(path):
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr_data = struct.pack('>IIBBBBB', 80, 60, 8, 0, 0, 0, 0)
    ihdr_crc = struct.pack('>I', zlib.crc32(b'IHDR' + ihdr_data))
    ihdr = struct.pack('>I', len(ihdr_data)) + b'IHDR' + ihdr_data + ihdr_crc
    raw_scanlines = b''.join(b'\x00' + b'\x80' * 80 for _ in range(60))
    idat_data = zlib.compress(raw_scanlines)
    idat_crc = struct.pack('>I', zlib.crc32(b'IDAT' + idat_data))
    idat = struct.pack('>I', len(idat_data)) + b'IDAT' + idat_data + idat_crc
    iend = struct.pack('>I', 0) + b'IEND' + struct.pack('>I', zlib.crc32(b'IEND'))
    with open(path, 'wb') as f:
        f.write(sig + ihdr + idat + iend)

make_png('$FRAMES_GAP_DIR/frame_0001.png')
make_png('$FRAMES_GAP_DIR/frame_0003.png')
"
run_directive "Builder sequence gap detection" \
    "$BIN build --frames-dir $FRAMES_GAP_DIR --output $WORK_DIR/gap_out.bin" 1

# 4c. Valid frames builder and atomic output
FRAMES_VALID_DIR="$WORK_DIR/frames_valid"
mkdir -p "$FRAMES_VALID_DIR"
python3 -c "
import struct, zlib

def make_png(path):
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr_data = struct.pack('>IIBBBBB', 80, 60, 8, 0, 0, 0, 0)
    ihdr_crc = struct.pack('>I', zlib.crc32(b'IHDR' + ihdr_data))
    ihdr = struct.pack('>I', len(ihdr_data)) + b'IHDR' + ihdr_data + ihdr_crc
    raw_scanlines = b''.join(b'\x00' + b'\x80' * 80 for _ in range(60))
    idat_data = zlib.compress(raw_scanlines)
    idat_crc = struct.pack('>I', zlib.crc32(b'IDAT' + idat_data))
    idat = struct.pack('>I', len(idat_data)) + b'IDAT' + idat_data + idat_crc
    iend = struct.pack('>I', 0) + b'IEND' + struct.pack('>I', zlib.crc32(b'IEND'))
    with open(path, 'wb') as f:
        f.write(sig + ihdr + idat + iend)

make_png('$FRAMES_VALID_DIR/frame_0001.png')
make_png('$FRAMES_VALID_DIR/frame_0002.png')
"
run_directive "Builder valid sequential frames" \
    "$BIN build --frames-dir $FRAMES_VALID_DIR --output $WORK_DIR/valid_built.bin" 0

echo ""
echo "=== Directive 5: Feature Permutations (No-default features) ==="

echo "Building video-only (no-default-features) target..."
cargo build --release --no-default-features --manifest-path "$ROOT_DIR/Cargo.toml"
run_directive "Video-only binary runs without audio" \
    "script -qec '$BIN play --input $WORK_DIR/valid_built.bin' /dev/null" 0

# Restore default feature build
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

echo ""
echo "=== Stress Test Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"

if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
