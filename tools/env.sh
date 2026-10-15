# Source this file (`. tools/env.sh`) before building or testing on Linux.
# The Rust toolchain is installed user-level by rustup and is not on the login PATH.
export PATH="$HOME/.cargo/bin:$PATH"

# Everything temporary stays inside the repository's gitignored scratch folder.
_root="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
export TMPDIR="$_root/scratch/tmp"
mkdir -p "$TMPDIR"

# The game client lives outside the repository. Override with your own path.
export AOMAC_CLIENT="${AOMAC_CLIENT:-$HOME/Games/ProjectRubiKa/client}"
unset _root
