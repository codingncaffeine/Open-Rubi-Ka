#!/usr/bin/env bash
# Checks that an installed Project Rubi-Ka client is the build this code was written against.
#   tools/prk/hash-check.sh [CLIENT_DIR]      (default: $AOMAC_CLIENT, else ~/Games/ProjectRubiKa/client)
# The expected hashes live in tools/prk/client-<version>.sha256 (sha256sum format, paths relative to the client root);
# the version comes from the client's own patch.version. Exit 0 = every listed file matches.
set -u
client="${1:-${AOMAC_CLIENT:-$HOME/Games/ProjectRubiKa/client}}"
here="$(cd "$(dirname "$0")" && pwd)"
[ -d "$client" ] || { echo "hash-check: no client at $client" >&2; exit 2; }
version="$(tr -d '\r\n' < "$client/patch.version" 2>/dev/null)"
[ -n "$version" ] || { echo "hash-check: $client/patch.version missing or empty" >&2; exit 2; }
list="$here/client-$version.sha256"
[ -f "$list" ] || { echo "hash-check: no expected hashes for client version $version ($list)" >&2; exit 3; }
echo "client $client, version $version ($(tr -d '\r\n' < "$client/version.id" 2>/dev/null))"
cd "$client" && sha256sum --quiet -c "$list" && echo "hash-check OK: $(wc -l < "$list") files match"
