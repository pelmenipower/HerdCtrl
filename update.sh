#!/usr/bin/env bash
# HerdCtrl update wrapper: runs the installed herdctrl-update, or the verified installer of the latest release.
#   curl -fsSL https://github.com/pelmenipower/HerdCtrl/releases/latest/download/update.sh | bash
set -euo pipefail

readonly REPO_URL=https://github.com/pelmenipower/HerdCtrl

# Global so the EXIT trap can still see it after main returned (set -u).
work=''
cleanup() {
    if [[ -n $work ]]; then
        rm -rf -- "$work"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    printf 'HerdCtrl: %s\n' "$*" >&2
    exit 1
}

download() { # download <url> <file>
    curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
        --connect-timeout 20 --max-time 600 -o "$2" "$1" </dev/null
}

main() {
    local bin version url='' expected status=0
    [[ -n ${HOME:-} ]] || fail 'HOME is not set.'
    if [[ ${XDG_BIN_HOME:-} == /* ]]; then bin=${XDG_BIN_HOME%/}; else bin=$HOME/.local/bin; fi
    if [[ -x $bin/herdctrl-update ]]; then
        exec "$bin/herdctrl-update" "$@"
    fi

    local tool
    for tool in curl sha256sum awk mktemp bash; do
        command -v "$tool" >/dev/null 2>&1 || fail "required program missing: $tool"
    done
    url=$(curl --fail --silent --show-error --proto '=https' --max-time 30 -o /dev/null -w '%{redirect_url}' "$REPO_URL/releases/latest" </dev/null) || url=''
    if [[ $url =~ ^https://github\.com/pelmenipower/HerdCtrl/releases/tag/v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
        version=${BASH_REMATCH[1]}
    else
        fail "could not determine the latest release (GitHub answered: ${url:-no redirect})."
    fi

    work=$(mktemp -d -t herdctrl-update.XXXXXXXX)
    download "$REPO_URL/releases/download/v$version/SHA256SUMS" "$work/SHA256SUMS" || fail 'could not download SHA256SUMS.'
    download "$REPO_URL/releases/download/v$version/install.sh" "$work/install.sh" || fail 'could not download install.sh.'
    expected=$(awk '$2 == "install.sh" { print $1; n++ } END { exit n != 1 }' "$work/SHA256SUMS") ||
        fail 'no unique checksum for install.sh in SHA256SUMS. Nothing was installed.'
    [[ $expected =~ ^[a-fA-F0-9]{64}$ ]] || fail 'invalid checksum for install.sh in SHA256SUMS. Nothing was installed.'
    printf '%s  %s\n' "$expected" "$work/install.sh" | sha256sum --check --status ||
        fail 'checksum mismatch for install.sh. Nothing was installed.'

    bash "$work/install.sh" "$@" </dev/null || status=$?
    exit "$status"
}

main "$@"; exit
