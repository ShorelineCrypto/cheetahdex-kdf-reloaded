#!/usr/bin/env bash
# with-test-passphrases.sh — run a command with throwaway BOB/ALICE test seeds.
#
# Why this exists
# ---------------
# The docker UTXO integration tests run two swap counterparties ("Bob" the
# maker/seed and "Alice" the taker/client) and read their wallet passphrases
# from the BOB_PASSPHRASE / ALICE_PASSPHRASE environment variables. The docker
# harness funds the addresses *derived from* those passphrases dynamically on a
# private regtest chain it spins up (see
# mm2src/mm2_main/src/docker_tests/docker_tests_common.rs::fill_address), so the
# exact passphrase value is irrelevant — only that the two are non-empty and
# distinct. That means we do NOT have to hard-code (and commit) any specific seed
# for this job.
#
# This wrapper generates a fresh, random, throwaway passphrase pair for each run,
# exports them, and then execs the command passed as arguments. Nothing secret
# is stored in the repository. See docs/TEST_ENV_VARS.md for the full picture.
#
# Usage
# -----
#   scripts/with-test-passphrases.sh cargo test --bin docker_tests ...
#
# Reproducible / fixed-seed override
# ----------------------------------
# If BOB_PASSPHRASE and/or ALICE_PASSPHRASE are already set and non-empty in the
# environment, they are respected as-is. This lets a developer reproduce a run
# with specific seeds, and lets the live-testnet jobs (which withdraw from
# *pre-funded* addresses) keep their fixed, well-known public testnet seeds.
#
# Scope / caveat
# --------------
# These random seeds only work for tests that fund addresses dynamically (the
# docker regtest suite, via the legacy "iguana" passphrase path, which accepts
# any non-empty string). Tests that withdraw from a *pre-funded* live-testnet
# address (e.g. the DOC/MARTY withdraw tests) require their specific funded seed
# and must NOT use random values — set BOB_PASSPHRASE/ALICE_PASSPHRASE explicitly
# for those instead.
set -euo pipefail

# Generate a 12-token hex string, shaped like a mnemonic, guaranteed non-empty
# and effectively unique per call. Uses /dev/urandom only — no openssl or python
# dependency, so it runs anywhere bash does.
gen_passphrase() {
  local out="" i tok
  for i in $(seq 1 12); do
    tok=$(head -c 4 /dev/urandom | od -An -tx1 | tr -d ' \n')
    out="${out:+$out }$tok"
  done
  printf '%s' "$out"
}

# Short, non-revealing fingerprint for log debuggability. The seeds are
# throwaway and fund only ephemeral regtest balances, but we still avoid dumping
# the full value into CI logs out of habit.
fingerprint() {
  printf '%s' "$1" | cksum | cut -d' ' -f1
}

if [ -z "${BOB_PASSPHRASE:-}" ]; then
  BOB_PASSPHRASE="$(gen_passphrase)"
fi
if [ -z "${ALICE_PASSPHRASE:-}" ]; then
  ALICE_PASSPHRASE="$(gen_passphrase)"
fi

# The two counterparties must be distinct; regenerate Alice on the
# astronomically unlikely collision.
while [ "$BOB_PASSPHRASE" = "$ALICE_PASSPHRASE" ]; do
  ALICE_PASSPHRASE="$(gen_passphrase)"
done

export BOB_PASSPHRASE ALICE_PASSPHRASE

echo "with-test-passphrases: BOB fp=$(fingerprint "$BOB_PASSPHRASE") ALICE fp=$(fingerprint "$ALICE_PASSPHRASE")"

if [ "$#" -eq 0 ]; then
  echo "with-test-passphrases: no command given; passphrases were generated but" \
       "are only visible to a child command. Pass a command to run, e.g." \
       "'scripts/with-test-passphrases.sh cargo test ...'." >&2
  exit 0
fi

exec "$@"
