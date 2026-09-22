#!/usr/bin/env bash
set -euo pipefail

# Comprehensive stress-test suite for bad_apple_rs across all directives.
# Directives tested:
# 1. Corrupt and malformed binary frame streams
# 2. Extreme CLI bounds (subnormal, negative, zero, non-finite FPS)
# 3. Missing asset fallbacks (audio and binary frames)
# 4. Sequence gap detection and builder invariants
# 5. Terminal signal recovery (SIGINT) and non-blocking abort
# 6. Feature permutations (default audio vs pure Rust video-only)
#
# Requirements: Bash + python3 (stdlib `pty` for portable PTY allocation).
# Paths must not contain newlines.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT_DIR/target/release/bad_apple"
WORK_DIR="$(mktemp -d /tmp/bad_apple_stress_XXXXXX)"
PASS=0
FAIL=0

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 not found on PATH (needed for PTY play directives and PNG fixtures)." >&2
    exit 127
fi

# Portable PTY runner: argv after -c is the command to spawn (Linux + macOS/BSD).
# Prefer os.waitstatus_to_exitcode (3.9+); fall back to WIF* helpers.
PTY_RUN=$'import os, pty, sys\nargv = sys.argv[1:]\nstatus = pty.spawn(argv)\nif hasattr(os, "waitstatus_to_exitcode"):\n    code = os.waitstatus_to_exitcode(status)\nelif os.WIFSIGNALED(status):\n    code = 128 + os.WTERMSIG(status)\nelif os.WIFEXITED(status):\n    code = os.WEXITSTATUS(status)\nelse:\n    code = 1\nsys.exit(code)\n'

echo "=== Building bad_apple binary in release mode ==="
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

run_directive() {
    local name="$1"
    local expected_exit="$2"
    shift 2

    echo -n "Directive: $name ... "
    set +e
    "$@" >"$WORK_DIR/out.log" 2>"$WORK_DIR/err.log"
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

# Run a play command under a portable Python PTY (no util-linux `script` / shell escaping).
run_play_pty() {
    local name="$1"
    local expected_exit="$2"
    shift 2
    run_directive "$name" "$expected_exit" python3 -c "$PTY_RUN" "$@"
}

echo ""
echo "=== Directive 1: Input Frame Stream Boundaries ==="

# 1a. Empty binary file (0 bytes) -> clean EOF
touch "$WORK_DIR/empty.bin"
run_play_pty "0-byte binary frame EOF" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin"

# 1b. Truncated 1 byte
head -c 1 /dev/urandom >"$WORK_DIR/trunc1.bin"
run_play_pty "1-byte truncated frame reject" 1 \
    "$BIN" play --input "$WORK_DIR/trunc1.bin"

# 1c. Truncated 4799 bytes (1 byte short of 1 frame)
head -c 4799 /dev/urandom >"$WORK_DIR/trunc4799.bin"
run_play_pty "4799-byte frame truncation reject" 1 \
    "$BIN" play --input "$WORK_DIR/trunc4799.bin"

# 1d. 4801 bytes (1 frame + 1 trailing byte)
head -c 4801 /dev/urandom >"$WORK_DIR/trunc4801.bin"
run_play_pty "4801-byte stream with trailing byte reject" 1 \
    "$BIN" play --input "$WORK_DIR/trunc4801.bin"

# 1e. Nonexistent input file
run_play_pty "Nonexistent input file reject" 1 \
    "$BIN" play --input "$WORK_DIR/does_not_exist.bin"

echo ""
echo "=== Directive 2: Extreme CLI Bounds and FPS Normalization ==="

# 2a. Zero FPS
run_play_pty "Zero FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps 0

# 2b. Negative FPS
run_play_pty "Negative FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps -30

# 2c. Tiny finite FPS
run_play_pty "Tiny FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps 1e-30

# 2d. True subnormal FPS
run_play_pty "Subnormal FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps 1e-320

# 2e. Huge FPS
run_play_pty "Astronomical FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps 9999999999

# 2f/2g. Non-finite FPS (parser-supported)
run_play_pty "Infinite FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps inf

run_play_pty "NaN FPS normalization" 0 \
    "$BIN" play --input "$WORK_DIR/empty.bin" --fps NaN

echo ""
echo "=== Directive 3: Missing Audio Fallback (deferred until valid frames) ==="

echo ""
echo "=== Directive 4: Frame Sequence Builder Invariants ==="

# 4a. Missing frames directory
run_directive "Builder missing directory error" 1 \
    "$BIN" build --frames-dir "$WORK_DIR/nonexistent_dir" --output "$WORK_DIR/out.bin"

# 4b/4c. Shared PNG helper — create both gap and valid frame directories once
FRAMES_GAP_DIR="$WORK_DIR/frames_gap"
FRAMES_VALID_DIR="$WORK_DIR/frames_valid"
mkdir -p "$FRAMES_GAP_DIR" "$FRAMES_VALID_DIR"
python3 -c "
import struct, zlib, sys

def make_png(path):
    sig = b'\\x89PNG\\r\\n\\x1a\\n'
    ihdr_data = struct.pack('>IIBBBBB', 80, 60, 8, 0, 0, 0, 0)
    ihdr_crc = struct.pack('>I', zlib.crc32(b'IHDR' + ihdr_data) & 0xffffffff)
    ihdr = struct.pack('>I', len(ihdr_data)) + b'IHDR' + ihdr_data + ihdr_crc
    raw_scanlines = b''.join(b'\\x00' + b'\\x80' * 80 for _ in range(60))
    idat_data = zlib.compress(raw_scanlines)
    idat_crc = struct.pack('>I', zlib.crc32(b'IDAT' + idat_data) & 0xffffffff)
    idat = struct.pack('>I', len(idat_data)) + b'IDAT' + idat_data + idat_crc
    iend = struct.pack('>I', 0) + b'IEND' + struct.pack('>I', zlib.crc32(b'IEND') & 0xffffffff)
    with open(path, 'wb') as f:
        f.write(sig + ihdr + idat + iend)

gap_dir, valid_dir = sys.argv[1], sys.argv[2]
make_png(f'{gap_dir}/frame_0001.png')
make_png(f'{gap_dir}/frame_0003.png')
make_png(f'{valid_dir}/frame_0001.png')
make_png(f'{valid_dir}/frame_0002.png')
" "$FRAMES_GAP_DIR" "$FRAMES_VALID_DIR"

run_directive "Builder sequence gap detection" 1 \
    "$BIN" build --frames-dir "$FRAMES_GAP_DIR" --output "$WORK_DIR/gap_out.bin"

run_directive "Builder valid sequential frames" 0 \
    "$BIN" build --frames-dir "$FRAMES_VALID_DIR" --output "$WORK_DIR/valid_built.bin"

echo ""
echo "=== Directive 3b: Missing Audio Fallback (real frames) ==="
run_play_pty "Missing audio warns and continues" 0 \
    "$BIN" play --input "$WORK_DIR/valid_built.bin" --audio "$WORK_DIR/missing.ogg"
# Under pty.spawn, app stderr is typically copied onto the parent stdout stream (out.log).
if ! grep -Eiq "Could not open audio|Failed to initialize audio|Failed to decode audio|Failed to create audio|audio" "$WORK_DIR/out.log" "$WORK_DIR/err.log"; then
    echo "Directive: Missing audio warning present in PTY output ... FAIL"
    FAIL=$((FAIL + 1))
else
    echo "Directive: Missing audio warning present in PTY output ... PASS"
    PASS=$((PASS + 1))
fi

echo ""
echo "=== Directive 5: Terminal signal recovery (SIGINT) ==="
echo -n "Directive: SIGINT aborts playback ... "
set +e
# Use MIN_FPS so the 2-frame fixture stays alive long enough to interrupt.
# SIGINT must hit the bad_apple child (ctrlc handler). Killing the PTY parent alone can exit 0.
python3 -c "$PTY_RUN" "$BIN" play --input "$WORK_DIR/valid_built.bin" --fps 0.001 \
    >"$WORK_DIR/sig_out.log" 2>"$WORK_DIR/sig_err.log" &
ppid=$!
bpid=""
for _ in $(seq 1 40); do
    # pgrep -P works on Linux and macOS; prefer the direct child of the python PTY runner.
    cand=$(pgrep -P "$ppid" 2>/dev/null | head -1 || true)
    if [[ -n "${cand:-}" ]]; then
        bpid=$cand
        break
    fi
    sleep 0.05
done
if [[ -n "${bpid:-}" ]]; then
    kill -INT "$bpid" 2>/dev/null || true
else
    kill -INT "$ppid" 2>/dev/null || true
fi
wait "$ppid"
sig_code=$?
set -e
# 130 = interrupt path from main / waitstatus_to_exitcode; also accept aborted message in PTY output
if [[ "$sig_code" -eq 130 ]] || grep -Eq "Playback aborted by user interrupt" "$WORK_DIR/sig_out.log" "$WORK_DIR/sig_err.log" 2>/dev/null; then
    echo "PASS (exit $sig_code)"
    PASS=$((PASS + 1))
else
    echo "FAIL (exit $sig_code)"
    tail -c 400 "$WORK_DIR/sig_out.log" || true
    FAIL=$((FAIL + 1))
fi

echo ""
echo "=== Directive 6: Feature Permutations (No-default features) ==="

echo "Building video-only (no-default-features) target..."
cargo build --release --no-default-features --manifest-path "$ROOT_DIR/Cargo.toml"
run_play_pty "Video-only binary runs without audio" 0 \
    "$BIN" play --input "$WORK_DIR/valid_built.bin"

# Restore default feature build
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

echo ""
echo "=== Stress Test Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"

if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
