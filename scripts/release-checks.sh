#!/usr/bin/env bash
# Pre-release validation gate: type-check, frontend unit tests, cargo check, cargo test.
#
# Run before `make release` bumps version files/commits/tags, so a broken
# build fails fast locally instead of surfacing after a tag is already
# pushed to CI (which means: delete tag, delete draft release, fix, re-tag).
#
# Kept as a standalone script (rather than inlined in the Makefile release
# recipe) so CI can call it too later without duplicating the logic — not
# wired into any GitHub Actions workflow yet.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "release-checks: frontend type-check + unit tests + cargo check (make check)"
make check

echo "release-checks: cargo test (make test)"
make test

echo "release-checks: all checks passed"
