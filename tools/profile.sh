#!/bin/bash
set -euo pipefail

cargo build --profile release-fast
samply record ./target/release-fast/clawseed "$@"
