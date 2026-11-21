#!/usr/bin/env bash
# Fetches GridCraft at the tag in upstream.pin into upstream/gridcraft, checks the commit, and
# applies patches/*.patch in order. Safe to re-run: it starts from a clean checkout every time.
set -euo pipefail
cd "$(dirname "$0")/.."
tag=$(sed -n 's/^tag = "\(.*\)"/\1/p' upstream.pin)
commit=$(sed -n 's/^commit = "\(.*\)"/\1/p' upstream.pin)
repo=$(sed -n 's/^repo = "\(.*\)"/\1/p' upstream.pin)
dir=upstream/gridcraft
if [ ! -d "$dir/.git" ]; then
  mkdir -p upstream
  git clone --quiet --depth 1 --branch "$tag" "$repo" "$dir"
else
  git -C "$dir" fetch --quiet --depth 1 origin "refs/tags/$tag:refs/tags/$tag"
  git -C "$dir" checkout --quiet --force "$tag"
  git -C "$dir" clean --quiet -fdx
fi
actual=$(git -C "$dir" rev-parse HEAD)
[ "$actual" = "$commit" ] || { echo "upstream $tag is $actual, expected $commit (tag moved?)" >&2; exit 1; }
for p in patches/*.patch; do
  [ -e "$p" ] || continue
  git -C "$dir" apply --whitespace=nowarn "../../$p" || { echo "patch failed: $p" >&2; exit 1; }
  echo "applied $p"
done
# Our [patch.crates-io] block must match upstream's (egui fork pins).
ours=$(sed -n '/^# Keep the complete egui family/,$p' Cargo.toml)
theirs=$(sed -n '/^# Keep the complete egui family/,$p' "$dir/Cargo.toml")
[ "$ours" = "$theirs" ] || { echo "Cargo.toml [patch.crates-io] differs from upstream's: copy it over" >&2; exit 1; }
echo "upstream ready: GridCraft $tag ($commit)"
