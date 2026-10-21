#!/bin/sh
# Generates throwaway client keys in ./keys (git-ignored). Requires ssh-keygen; puttygen is optional.
set -e
cd "$(dirname "$0")"
mkdir -p keys
gen() { [ -f "keys/$1" ] || ssh-keygen -q -t "$2" $3 -N "$4" -C "mdm-test-$1" -f "keys/$1"; }
gen id_ed25519        ed25519 ""          ""
gen id_ed25519_enc    ed25519 ""          "test-passphrase"
gen id_rsa            rsa     "-b 3072"   ""
if command -v puttygen >/dev/null 2>&1; then
  [ -f keys/id_rsa_v2.ppk ] || puttygen keys/id_rsa -O private --ppk-param version=2 -o keys/id_rsa_v2.ppk
  [ -f keys/id_ed25519_v3_enc.ppk ] || { echo test-passphrase > keys/.pp; puttygen keys/id_ed25519 -O private --new-passphrase keys/.pp -o keys/id_ed25519_v3_enc.ppk; rm keys/.pp; }
fi
ls keys
