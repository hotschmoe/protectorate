#!/usr/bin/env bash
# The shared installer lives beside this wrapper so both roles use one policy.
set -euo pipefail
shared="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/install-hub.sh"
[[ -f "$shared" && ! -L "$shared" ]] || { printf 'install-node: install-hub.sh must be beside install-node.sh\n' >&2; exit 1; }
# shellcheck source=install-hub.sh
source "$shared"
main --role node "$@"
