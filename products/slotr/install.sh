#!/bin/sh

main() {
    VERSION=${SLOTR_VERSION:-}
    REPO=olafurns7/herdr-kit
    fail() { printf 'slotr install: %s\n' "$*" >&2; exit 1; }
    config_requested=false
    case "$#:${1:-}" in
        0:) ;;
        1:--config) config_requested=true ;;
        *) printf 'usage: sh install.sh [--config]\n' >&2; return 2 ;;
    esac
    export LC_ALL=C
    [ "$(uname -s) $(uname -m)" = "Linux x86_64" ] || fail "Linux x86_64 only"
    for tool in curl tar sha256sum; do
        command -v "$tool" >/dev/null 2>&1 || fail "missing $tool"
    done
    [ -n "${HOME:-}" ] || fail "HOME is not set"
    download_dir=$(mktemp -d 2>/dev/null) || fail "cannot create temporary directory"
    install_tmp=
    trap 'rm -f "$install_tmp"; rm -rf "$download_dir"' 0
    trap 'fail "interrupted"' HUP INT TERM
    transport=public
    if [ -z "${SLOTR_INSTALL_BASE_URL:-}" ] && command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
        transport=gh
    fi
    if [ -n "$VERSION" ]; then
        VERSION=${VERSION#slotr/}
        VERSION=${VERSION#v}
        printf '%s\n' "$VERSION" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' || fail 'invalid SLOTR_VERSION'
    elif [ -n "${SLOTR_INSTALL_BASE_URL:-}" ]; then
        # shortcut: mirrors default to 0.2.0; set SLOTR_VERSION for a newer mirror.
        VERSION=0.2.0
    else
        config_assets='[]'
        if "$config_requested"; then config_assets='["config.example.toml"]'; fi
        filter='.[]
            | select(.draft == false and .prerelease == false)
            | select(.tag_name | test("^slotr/v(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$"))
            | (.tag_name | ltrimstr("slotr/v")) as $version
            | ("slotr-" + $version + "-x86_64-unknown-linux-musl.tar.gz") as $asset
            | select(([.assets[].name] as $assets | ([$asset, ($asset + ".sha256")] + '"$config_assets"' - $assets) | length) == 0)
            | $version'
        if [ "$transport" = gh ]; then
            gh api --paginate "repos/$REPO/releases?per_page=100" --jq "$filter" > "$download_dir/versions" || fail 'could not list releases'
        else
            command -v jq >/dev/null 2>&1 || fail 'jq is required for unpinned installs; set SLOTR_VERSION to pin a release'
            page=1
            : > "$download_dir/versions"
            while :; do
                curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=100&page=$page" -o "$download_dir/page.json" || fail 'could not list releases'
                jq -r "$filter" "$download_dir/page.json" >> "$download_dir/versions" || fail 'invalid release metadata'
                count=$(jq 'length' "$download_dir/page.json") || fail 'invalid release metadata'
                [ "$count" -eq 100 ] || break
                page=$((page + 1))
            done
        fi
        VERSION=$(sort -t. -k1,1n -k2,2n -k3,3n "$download_dir/versions" | tail -n 1)
        [ -n "$VERSION" ] || fail 'no stable slotr release has the required assets'
    fi
    tag=slotr/v$VERSION
    asset=slotr-$VERSION-x86_64-unknown-linux-musl.tar.gz
    base=${SLOTR_INSTALL_BASE_URL:-https://github.com/$REPO/releases/download/$tag}
    download() {
        if [ "$transport" = gh ]; then
            gh release download "$tag" --repo "$REPO" --pattern "$1" --dir "$download_dir" --clobber
        else
            curl -fsSL "$base/$1" -o "$download_dir/$1"
        fi
    }
    download "$asset" 2>/dev/null && download "$asset.sha256" 2>/dev/null || fail "the release download failed"
    hash=$(sha256sum <"$download_dir/$asset" 2>/dev/null) || fail "the checksum did not verify"
    hash=${hash%% *}
    checksum=$(cat "$download_dir/$asset.sha256" 2>/dev/null) || fail "the checksum did not verify"
    { [ "${#hash}" -eq 64 ] && [ "$(grep -c '' <"$download_dir/$asset.sha256")" = 1 ] &&
        { [ "$checksum" = "$hash  $asset" ] || [ "$checksum" = "$hash *$asset" ]; }; } || fail "the checksum did not verify"
    members=$(tar -tvzf "$download_dir/$asset" 2>/dev/null) || fail "the archive layout is invalid"
    printf '%s\n' "$members" | awk '
        $1 !~ /^-/ || NF != 6 || ($6 != "slotr" && $6 != "LICENSE" && $6 != "THIRD-PARTY-NOTICES") {bad=1}
        {if (seen[$6]++) bad=1}
        $6 == "slotr" {binary++}
        END {exit bad || binary != 1}
    ' || fail "the archive layout is invalid"
    { mkdir "$download_dir/bin" && tar -xzf "$download_dir/$asset" -C "$download_dir/bin" slotr &&
        [ -f "$download_dir/bin/slotr" ] && [ ! -L "$download_dir/bin/slotr" ]; } 2>/dev/null || fail "the archive holds no slotr binary"
    binary=$HOME/.local/bin/slotr
    mkdir -p "$HOME/.local/bin" 2>/dev/null || fail "cannot write ~/.local/bin/slotr"
    install_tmp=$(mktemp "$HOME/.local/bin/.slotr.XXXXXX" 2>/dev/null) || fail "cannot write ~/.local/bin/slotr"
    { cat "$download_dir/bin/slotr" >"$install_tmp" && chmod 0755 "$install_tmp" &&
        mv -fT "$install_tmp" "$binary"; } 2>/dev/null || fail "cannot write ~/.local/bin/slotr"
    "$binary" --version 2>/dev/null || fail "slotr --version failed"
    printf 'Installed to %s\n' "$binary"
    case ":${PATH:-}:" in
        *":$HOME/.local/bin:"*) ;;
        *) printf 'Add ~/.local/bin to PATH: export PATH="$HOME/.local/bin:$PATH"\n' ;;
    esac
    if "$config_requested"; then
        config=$HOME/.config/slotr/config.toml
        mkdir -p "$HOME/.config/slotr" 2>/dev/null || fail "cannot create config directory"
        if [ -e "$config" ] || [ -L "$config" ]; then
            printf 'kept existing config\n'
        else
            download config.example.toml 2>/dev/null || fail "the config download failed"
            (set -C; cat "$download_dir/config.example.toml" >"$config") 2>/dev/null || fail "cannot write config"
        fi
        "$binary" config check "$config" 2>/dev/null || fail "slotr config check failed"
    fi
}

main "$@"
