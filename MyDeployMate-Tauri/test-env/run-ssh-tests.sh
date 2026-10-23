#!/bin/sh
# Runs the SSH integration tests (crates/exec/tests/ssh_test_env.rs) against the Docker targets.
# Needs docker, ssh-keygen, ssh-agent and (for the .ppk tests) puttygen.
set -e
cd "$(dirname "$0")"
./gen-keys.sh >/dev/null
docker compose up -d --build
for port in 2222 2223; do
  i=0
  until ssh-keyscan -p "$port" 127.0.0.1 >/dev/null 2>&1; do
    i=$((i + 1)); [ "$i" -lt 30 ] || { echo "sshd on $port did not start" >&2; exit 1; }
    sleep 1
  done
done
eval "$(ssh-agent -s)" >/dev/null
trap 'ssh-agent -k >/dev/null' EXIT
ssh-add -q keys/id_ed25519
export MDM_TEST_ENV_KEYS="$PWD/keys" MDM_TEST_PASSWORD=deploy-pass
cd ..
cargo test -p mdm-exec --test ssh_test_env "$@" -- --nocapture
