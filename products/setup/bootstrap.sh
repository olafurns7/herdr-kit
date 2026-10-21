#!/bin/sh
set -eu

for tool in gh tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'ERROR: %s is required; install it and retry.\n' "$tool" >&2
        exit 1
    fi
done

bootstrap_dir=$(mktemp -d "${TMPDIR:-/tmp}/herdr-setup-bootstrap.XXXXXX")
trap 'rm -rf "${bootstrap_dir:?}"' 0
trap 'exit 1' 1 2 3 15

tag=${TASKR_VERSION:-taskr/v0.26.0}
tag=${tag#taskr/}; tag=${tag#v}
printf '%s\n' "$tag" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || {
    printf '%s\n' 'ERROR: invalid TASKR_VERSION' >&2; exit 2;
}
tag=taskr/v$tag
gh api "repos/olafurns7/herdr-kit/tarball/$tag" >"$bootstrap_dir/source.tar.gz"
mkdir "$bootstrap_dir/source"
tar -xzf "$bootstrap_dir/source.tar.gz" -C "$bootstrap_dir/source" --strip-components=1
if [ ! -f "$bootstrap_dir/source/products/taskr/install.sh" ]; then
    printf '%s\n' 'ERROR: downloaded source has no regular install.sh.' >&2
    exit 1
fi

TASKR_SOURCE_TAG=$tag sh "$bootstrap_dir/source/products/taskr/install.sh" --fleet "$@"
