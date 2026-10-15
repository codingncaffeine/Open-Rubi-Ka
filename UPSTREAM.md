# Upstream

Open Rubi-Ka is a fork of [aorust](https://github.com/reiserFSs/aorust) by reiserFSs: a native reimplementation of
the Anarchy Online classic client in the build Project Rubi-Ka ships, written for macOS. This fork targets Linux
first and Windows second and adds high-resolution and ultrawide display support.

- Upstream declares its licence in `Cargo.toml` (`[workspace.package] license = "MIT"`) and ships no licence file;
  `LICENSE` here carries the MIT text with upstream's copyright line and ours.
- The full upstream history is this repository's history. Remote: `git remote add upstream
  https://github.com/reiserFSs/aorust.git`; a sync is `git fetch upstream && git merge upstream/main`, recorded below.
- Upstream's own documentation lives in `docs/` and still uses upstream's names (`aomac`, macOS paths) where this
  fork has not changed the behaviour yet.

| Date | Upstream commit | Notes |
|---|---|---|
| 2026-10-15 | `c841d93` | Initial import (529 commits), merged with this repository's initial commit as `202e1aa`. No code changed. |
