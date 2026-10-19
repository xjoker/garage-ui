#!/bin/sh
# Verify release assets the way a user downloads them.
#
#   scripts/verify-release-assets.sh DIR VERSION   download every asset of release VERSION
#                                                   (draft or published) into the new or empty
#                                                   DIR with gh, then verify DIR
#   scripts/verify-release-assets.sh DIR           verify the files already in DIR
#
# Fails unless the names listed in SHA256SUMS plus SHA256SUMS itself are exactly the files in DIR,
# and `sha256sum --check --strict SHA256SUMS` passes. No --ignore-missing: a listed name that is
# not an asset (GitHub renames uploaded dotfiles) or an asset that is not listed is an error.
set -eu

dir=${1:?usage: verify-release-assets.sh DIR [VERSION]}
version=${2:-}
repo=${GITHUB_REPOSITORY:-xjoker/garage-ui}

if [ -n "$version" ]; then
  mkdir -p "$dir"
  if [ -n "$(ls -A "$dir")" ]; then
    echo "verify-release-assets: $dir must be empty before downloading" >&2
    exit 1
  fi
  gh release download "$version" --repo "$repo" --dir "$dir"
fi

if [ ! -f "$dir/SHA256SUMS" ]; then
  echo "verify-release-assets: no SHA256SUMS in $dir" >&2
  exit 1
fi

listed=$(mktemp)
present=$(mktemp)
trap 'rm -f "$listed" "$present"' EXIT HUP INT TERM
# Checksum lines are "<64 hex>  <name>" (or " *<name>" in binary mode).
{
  sed -E 's/^[0-9a-fA-F]{64} [ *]//' "$dir/SHA256SUMS"
  echo SHA256SUMS
} | LC_ALL=C sort > "$listed"
(cd "$dir" && find . -mindepth 1 -maxdepth 1 -printf '%P\n') | LC_ALL=C sort > "$present"
if ! cmp -s "$listed" "$present"; then
  echo "verify-release-assets: SHA256SUMS and the release assets name different files" >&2
  LC_ALL=C comm -23 "$listed" "$present" | sed 's/^/  listed in SHA256SUMS, not an asset: /' >&2
  LC_ALL=C comm -13 "$listed" "$present" | sed 's/^/  asset not listed in SHA256SUMS: /' >&2
  exit 1
fi
(cd "$dir" && sha256sum --check --strict SHA256SUMS)
echo "verify-release-assets: $(($(wc -l < "$listed") - 1)) assets match SHA256SUMS"
