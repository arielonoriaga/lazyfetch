#!/usr/bin/env bash
set -euo pipefail
if grep -RnE 'tokio::|std::fs::|std::net::|reqwest::|hyper::' crates/core/src; then
  echo "core must be IO-free"; exit 1
fi
