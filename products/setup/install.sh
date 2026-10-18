#!/bin/sh
# Compatibility entry point; taskr owns every placement.
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
exec sh "$script_dir/../taskr/install.sh" --fleet "$@"
