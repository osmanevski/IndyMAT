#!/bin/sh
# Emits every indy-graphics test figure and runs the UI's own admission gate
# (frontend/figure_data_utils.cjs validate()) over them with node.
#   motor/scripts/grafik_validate.sh [output-dir]
# Default output: ~/.cache/indymat-motor-fleet/grafik-validate (never /tmp).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
motor=$(cd "$here/.." && pwd)
out=${1:-"$HOME/.cache/indymat-motor-fleet/grafik-validate"}
: "${CARGO_HOME:=$HOME/.cache/indymat-motor-fleet/cargo-home}"
export CARGO_HOME
rm -rf "$out"
mkdir -p "$out"
(cd "$motor" && cargo run -q -p indy-graphics --example grafik_dump -- "$out")
node "$here/grafik_validate.cjs" "$out"
