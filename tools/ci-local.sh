#!/usr/bin/env bash
set -euo pipefail

# Local CI — mirrors .github/workflows/ci.yml
# Usage: ./tools/ci-local.sh [stage...]
#   Stages: fmt, clippy, check, build, test, coverage, all (default: all)
#   Example: ./tools/ci-local.sh fmt clippy test

BOLD='\033[1m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
RESET='\033[0m'

pass() { echo -e "${GREEN}✓ $1${RESET}"; }
fail() { echo -e "${RED}✗ $1${RESET}"; }
info() { echo -e "${BOLD}▸ $1${RESET}"; }
warn() { echo -e "${YELLOW}⚠ $1${RESET}"; }

FAILED=()

run_stage() {
    local name="$1"
    shift
    info "Running: $name"
    if "$@"; then
        pass "$name"
    else
        fail "$name"
        FAILED+=("$name")
    fi
    echo
}

# ── Dependency checks ─────────────────────────────────────────────
check_deps() {
    local missing=()

    if ! command -v cargo &>/dev/null; then
        echo -e "${RED}Error: cargo not found. Install Rust: https://rustup.rs${RESET}"
        exit 1
    fi

    if ! rustup component list --installed 2>/dev/null | grep -q rustfmt; then
        warn "rustfmt not installed, adding..."
        rustup component add rustfmt
    fi

    if ! rustup component list --installed 2>/dev/null | grep -q clippy; then
        warn "clippy not installed, adding..."
        rustup component add clippy
    fi

    if [[ " ${STAGES[*]} " =~ " test " ]] || [[ " ${STAGES[*]} " =~ " all " ]]; then
        if ! command -v cargo-nextest &>/dev/null; then
            warn "cargo-nextest not found, installing..."
            cargo install cargo-nextest --locked
        fi
    fi

    if [[ " ${STAGES[*]} " =~ " coverage " ]] || [[ " ${STAGES[*]} " =~ " all " ]]; then
        if ! command -v cargo-llvm-cov &>/dev/null; then
            warn "cargo-llvm-cov not found, installing..."
            rustup component add llvm-tools-preview
            cargo install cargo-llvm-cov --locked
        fi
    fi
}

# ── Stages ─────────────────────────────────────────────────────────
stage_fmt() {
    run_stage "fmt" cargo fmt --all -- --check
}

stage_clippy() {
    run_stage "clippy" cargo clippy --workspace --all-targets -- -D warnings
}

stage_check() {
    run_stage "check (all features)" cargo check --locked --all-features
    run_stage "check (no default features)" cargo check --locked --no-default-features
}

stage_build() {
    run_stage "build (ci profile)" cargo build --profile ci --locked
}

stage_test() {
    run_stage "test" cargo nextest run --locked --workspace
}

stage_coverage() {
    run_stage "coverage" cargo llvm-cov --workspace --lcov --output-path lcov.info
}

# ── Main ───────────────────────────────────────────────────────────
STAGES=("${@:-all}")

if [[ "${STAGES[*]}" == "all" ]]; then
    STAGES=(fmt clippy check build test)
fi

echo -e "${BOLD}ClawSeed Local CI${RESET}"
echo "Stages: ${STAGES[*]}"
echo

check_deps

for stage in "${STAGES[@]}"; do
    case "$stage" in
        fmt)      stage_fmt ;;
        clippy)   stage_clippy ;;
        check)    stage_check ;;
        build)    stage_build ;;
        test)     stage_test ;;
        coverage) stage_coverage ;;
        *)
            warn "Unknown stage: $stage"
            echo "Available: fmt, clippy, check, build, test, coverage, all"
            exit 1
            ;;
    esac
done

# ── Summary ────────────────────────────────────────────────────────
echo -e "${BOLD}── Summary ──${RESET}"
if [[ ${#FAILED[@]} -eq 0 ]]; then
    pass "All stages passed"
    exit 0
else
    fail "Failed stages: ${FAILED[*]}"
    exit 1
fi
