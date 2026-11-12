#!/usr/bin/env bash
# Downloads the world elevation grid from its online source (GEBCO, served by NOAA CoastWatch
# ERDDAP) and builds the 4-arc-minute pack the installers carry
# (docs/models/terrain-and-bathymetry.md, docs/build/releasing.md).
#
#   scripts/terrain/world-layer.sh <output folder>
#
# Environment (all optional):
#   GEBCO_DATASET  ERDDAP dataset id (default GEBCO_2020). Set a newer one to update the map.
#   GEBCO_CREDIT   the credit its licence asks for (default: the GEBCO 2020 credit).
#   ERDDAP         the ERDDAP server (default https://coastwatch.pfeg.noaa.gov/erddap).
#   NWS_BATCH      the nws-batch binary (default target/release/nws-batch).
#   GEBCO_FILE     use this already-downloaded netCDF file instead of downloading (offline
#                  rebuilds, tests). It must be the same query.
#
# Writes <name>.nwsterrain, its .sha256, and SOURCE.txt (the query, the download's SHA-256 and
# the date). The pack holds no date, so the same data always gives the same pack and pack id.
set -euo pipefail

out="${1:?usage: world-layer.sh <output folder>}"
dataset="${GEBCO_DATASET:-GEBCO_2020}"
credit="${GEBCO_CREDIT:-GEBCO Compilation Group (2020) GEBCO 2020 Grid (doi:10.5285/a29c5465-b138-234d-e053-6c86abc040b9)}"
server="${ERDDAP:-https://coastwatch.pfeg.noaa.gov/erddap}"
batch="${NWS_BATCH:-target/release/nws-batch}"
# GEBCO_2020 -> gebco2020-global-4m
name="$(printf '%s' "$dataset" | tr 'A-Z' 'a-z' | tr -d '_')-global-4m"

mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Every 8th 15-arc-second cell (2'), averaged 2 x 2 below to 4'. About 117 MB.
query="elevation[(-89.99):8:(89.99)][(-179.99):8:(179.99)]"
url="$server/griddap/$dataset.nc?$query"
if [ -n "${GEBCO_FILE:-}" ]; then
  echo "using $GEBCO_FILE (not downloaded)"
  cp "$GEBCO_FILE" "$work/world.nc"
else
  echo "downloading $url"
  # The server sometimes answers 503 while it prepares the subset: retry.
  curl -fgLsS --retry 5 --retry-all-errors --retry-delay 30 --max-time 3600 -o "$work/world.nc" "$url"
fi

if command -v sha256sum >/dev/null; then sum="sha256sum"; else sum="shasum -a 256"; fi
download_sha="$($sum "$work/world.nc" | cut -d' ' -f1)"

"$batch" terrain build "$work/world.nc" \
  --name "$name" --average 2 \
  --source "$dataset Grid, via NOAA CoastWatch ERDDAP, 2' subset averaged to 4'" \
  --credit "$credit" \
  --out "$out/$name.nwsterrain"

cat > "$out/SOURCE.txt" <<EOF
World terrain layer for the installers.
Query:      $url
Download:   $(wc -c < "$work/world.nc" | tr -d ' ') bytes, SHA-256 $download_sha
Built:      $(date -u +%Y-%m-%dT%H:%M:%SZ)
Credit:     $credit
EOF
cat "$out/SOURCE.txt"
cat "$out/$name.nwsterrain.sha256"
