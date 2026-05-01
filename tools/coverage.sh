#!/bin/bash
set -euo pipefail

cargo llvm-cov --workspace --html --output-dir coverage/
cargo llvm-cov --workspace --lcov --output-path lcov.info
echo "Coverage report: coverage/html/index.html"
