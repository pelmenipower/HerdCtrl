#!/usr/bin/env bash
# HerdCtrl installer and updater: Linux x86_64, per user, no root except for APT packages.
#   curl -fsSL https://github.com/pelmenipower/HerdCtrl/releases/latest/download/install.sh | bash
# Installed as ~/.local/bin/herdctrl-update. See docs/manual.md.
set -euo pipefail

readonly REPO_URL=https://github.com/pelmenipower/HerdCtrl
readonly MIN_VERSION=0.2.0
readonly EXT_UUID=herdctrl@herdctrl.local
readonly PACKAGES=(libxkbcommon-x11-0 libxcb-xkb1 libgl1 libvulkan1 mesa-vulkan-drivers wmctrl x11-xserver-utils)
readonly VERSION_RE='^[0-9]+\.[0-9]+\.[0-9]+$'
readonly SELF=${BASH_SOURCE[0]:-}
# Set by a handing-over installer. Taken into a shell variable and removed from the environment,
# so neither the restarted tray nor its children inherit it.
readonly handoff=${HERDCTRL_INSTALL_HANDOFF:-}
unset HERDCTRL_INSTALL_HANDOFF

# Global so the EXIT trap can still see it after main returned (set -u).
work=''
cleanup() {
    local status=$?
    if ((tray_pending)); then
        tray_pending=0
        warn 'The installation failed; restarting the tray.'
        find_procs
        if ((${#trays[@]} == 0)) && [[ -x $BIN/herdctrl ]]; then start_tray || true; fi
    fi
    if [[ -n $work ]]; then
        rm -rf -- "$work"
    fi
    return "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

orig_args=()
opt_version=''
opt_archive=''
opt_check=0
opt_reinstall=0
opt_no_packages=0
opt_extension=0
opt_no_extension=0
opt_uninstall=0
opt_purge=0

BIN=''
DATA=''
CONFIG=''
installed=unknown
version=''
stage=''
pkg=''
unpacked_version=''
trays=()
editors=()
killed=0
tray_pending=0

say() { printf '%s\n' "$*"; }
warn() { printf '%s\n' "$*" >&2; }
fail() {
    printf 'HerdCtrl: %s\n' "$*" >&2
    exit 1
}
usage_error() {
    printf 'HerdCtrl: %s\nRun with --help for usage.\n' "$*" >&2
    exit 2
}

usage() {
    cat <<'EOF'
Usage: install.sh [options]

Installs or updates HerdCtrl for the current user (~/.local/bin).

  --version X.Y.Z    install this release instead of the latest
  --archive PATH     install a local herdctrl-vX.Y.Z-linux-x86_64.tar.gz (no checksum check)
  --check            report installed and latest version; exit 10 if an update is available
  --reinstall        install even if this version is already installed
  --no-packages      do not install missing runtime packages with APT
  --extension        install the GNOME Shell extension even outside GNOME/Wayland
  --no-extension     do not install the GNOME Shell extension
  --uninstall        remove HerdCtrl (the configuration is kept)
  --purge            with --uninstall: also remove the configuration
  -h, --help         show this help

Exit codes: 0 success or nothing to do, 1 error, 2 usage error, 10 update available (--check).
EOF
}

parse_args() {
    while (($# > 0)); do
        case $1 in
            -h | --help)
                usage
                exit 0
                ;;
            --version)
                (($# >= 2)) && [[ -n $2 ]] || usage_error '--version needs a value'
                opt_version=${2#v}
                shift
                ;;
            --archive)
                (($# >= 2)) && [[ -n $2 ]] || usage_error '--archive needs a value'
                opt_archive=$2
                shift
                ;;
            --check) opt_check=1 ;;
            --reinstall) opt_reinstall=1 ;;
            --no-packages) opt_no_packages=1 ;;
            --extension) opt_extension=1 ;;
            --no-extension) opt_no_extension=1 ;;
            --uninstall) opt_uninstall=1 ;;
            --purge) opt_purge=1 ;;
            *) usage_error "unknown option: $1" ;;
        esac
        shift
    done
    if [[ -n $opt_version && -n $opt_archive ]]; then usage_error '--version and --archive cannot be combined'; fi
    if ((opt_check)) && [[ -n $opt_archive ]]; then usage_error '--check and --archive cannot be combined'; fi
    if ((opt_check && opt_reinstall)); then usage_error '--check and --reinstall cannot be combined'; fi
    if ((opt_check && opt_uninstall)); then usage_error '--check and --uninstall cannot be combined'; fi
    if ((opt_purge && !opt_uninstall)); then usage_error '--purge needs --uninstall'; fi
    if ((opt_uninstall)) && [[ -n $opt_version || -n $opt_archive ]] ||
        ((opt_uninstall && (opt_reinstall || opt_no_packages || opt_extension || opt_no_extension))); then
        usage_error '--uninstall only accepts --purge'
    fi
    if ((opt_extension && opt_no_extension)); then usage_error '--extension and --no-extension cannot be combined'; fi
    if [[ -n $opt_version && ! $opt_version =~ $VERSION_RE ]]; then
        usage_error "--version must look like X.Y.Z (got: $opt_version)"
    fi
}

# Same rule as the Rust code: an XDG variable counts only when it is an absolute path.
xdg_dir() {
    local value=$1 fallback=$2
    if [[ $value == /* ]]; then
        while [[ $value == */ && $value != / ]]; do value=${value%/}; done
        printf '%s' "$value"
    else
        printf '%s' "$fallback"
    fi
}

set_paths() {
    BIN=$(xdg_dir "${XDG_BIN_HOME:-}" "$HOME/.local/bin")
    DATA=$(xdg_dir "${XDG_DATA_HOME:-}" "$HOME/.local/share")
    CONFIG=$(xdg_dir "${XDG_CONFIG_HOME:-}" "$HOME/.config")
}

# $1 = XDG_CONFIG_HOME of a process, $2 = its HOME.
effective_config() { xdg_dir "$1" "$2/.config"; }

require_tools() {
    local tool
    for tool in "$@"; do
        command -v "$tool" >/dev/null 2>&1 || fail "required program missing: $tool"
    done
}

version_gt() {
    local -a a b
    local i IFS=.
    read -r -a a <<<"$1"
    read -r -a b <<<"$2"
    for i in 0 1 2; do
        if ((10#${a[i]:-0} > 10#${b[i]:-0})); then return 0; fi
        if ((10#${a[i]:-0} < 10#${b[i]:-0})); then return 1; fi
    done
    return 1
}

read_stamp() {
    local file=$DATA/herdctrl/installed-version line=''
    installed=unknown
    if [[ -f $file ]]; then
        IFS= read -r line <"$file" || true
        if [[ $line =~ $VERSION_RE ]]; then installed=$line; fi
    fi
}

latest_version() {
    local url=''
    url=$(curl --fail --silent --show-error --proto '=https' --max-time 30 -o /dev/null -w '%{redirect_url}' "$REPO_URL/releases/latest" </dev/null) || url=''
    if [[ $url =~ ^https://github\.com/pelmenipower/HerdCtrl/releases/tag/v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
        printf '%s' "${BASH_REMATCH[1]}"
    else
        fail "could not determine the latest release (GitHub answered: ${url:-no redirect})."
    fi
}

download() { # download <url> <file>
    curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
        --connect-timeout 20 --max-time 600 -o "$2" "$1" </dev/null
}

fetch() { # fetch <asset name> from the release of $version into $work
    download "$REPO_URL/releases/download/v$version/$1" "$work/$1" || fail "could not download $1."
}

acquire_lock() {
    exec 9>"${XDG_RUNTIME_DIR:-/tmp}/herdctrl-install-$(id -u).lock"
    flock -n 9 || fail 'Another HerdCtrl installer is running.'
}

gnome_wayland() {
    [[ ${XDG_CURRENT_DESKTOP:-} == *GNOME* && ${XDG_SESSION_TYPE:-} == wayland ]]
}

graphical_session() { [[ -n ${WAYLAND_DISPLAY:-}${DISPLAY:-} ]]; }

# ---- checksums -----------------------------------------------------------------------

expected_sum() { # expected_sum <asset>
    awk -v asset="$1" '$2 == asset { print $1; n++ } END { exit n != 1 }' "$work/SHA256SUMS"
}

verify_asset() {
    local name=$1 expected
    expected=$(expected_sum "$name") || fail "no unique checksum for $name in SHA256SUMS. Nothing was installed."
    [[ $expected =~ ^[a-fA-F0-9]{64}$ ]] || fail "invalid checksum for $name in SHA256SUMS. Nothing was installed."
    printf '%s  %s\n' "$expected" "$work/$name" | sha256sum --check --status ||
        fail "checksum mismatch for $name. Nothing was installed."
}

verify_signature() {
    if command -v cosign >/dev/null 2>&1; then
        fetch SHA256SUMS.sigstore.json
        cosign verify-blob --bundle "$work/SHA256SUMS.sigstore.json" \
            --certificate-identity "$REPO_URL/.github/workflows/release.yml@refs/tags/v$version" \
            --certificate-oidc-issuer https://token.actions.githubusercontent.com \
            "$work/SHA256SUMS" </dev/null >/dev/null 2>&1 ||
            fail 'the signature of SHA256SUMS does not verify. Nothing was installed.'
        if [[ -z $handoff ]]; then say 'Signature verified (Sigstore)'; fi
    elif [[ -z $handoff ]]; then
        say 'cosign not found: checksum only (see docs/manual.md#verify-a-download)'
    fi
}

# ---- processes -----------------------------------------------------------------------

# Fills trays/editors with our own HerdCtrl processes that use the same configuration.
find_procs() {
    trays=()
    editors=()
    local mine dir pid entry home xdg
    local -a argv
    mine=$(effective_config "${XDG_CONFIG_HOME:-}" "$HOME")
    for dir in /proc/[0-9]*; do
        pid=${dir#/proc/}
        if [[ $pid == "$$" ]]; then continue; fi
        if [[ ! -O $dir ]]; then continue; fi
        argv=()
        { mapfile -d '' -t argv <"$dir/cmdline"; } 2>/dev/null || continue
        if ((${#argv[@]} < 1)) || [[ ${argv[0]##*/} != herdctrl ]]; then continue; fi
        if ((${#argv[@]} == 2)) && [[ ${argv[1]} == --tray ]]; then
            :
        elif ((${#argv[@]} != 1)); then
            continue
        fi
        home=''
        xdg=''
        {
            while IFS= read -r -d '' entry; do
                case $entry in
                    HOME=*) home=${entry#HOME=} ;;
                    XDG_CONFIG_HOME=*) xdg=${entry#XDG_CONFIG_HOME=} ;;
                esac
            done <"$dir/environ"
        } 2>/dev/null || continue
        if [[ $(effective_config "$xdg" "$home") != "$mine" ]]; then continue; fi
        if ((${#argv[@]} == 2)); then trays+=("$pid"); else editors+=("$pid"); fi
    done
}

proc_gone() {
    local pid=$1 line rest
    if [[ ! -d /proc/$pid ]]; then return 0; fi
    { IFS= read -r line <"/proc/$pid/stat"; } 2>/dev/null || return 0
    rest=${line##*) }
    [[ ${rest%% *} == Z ]]
}

wait_gone() { # wait_gone <pid> <tenths of a second>
    local i
    for ((i = 0; i < $2; i++)); do
        if proc_gone "$1"; then return 0; fi
        sleep 0.1
    done
    proc_gone "$1"
}

stop_trays() {
    local pid
    killed=0
    for pid in "${trays[@]}"; do
        kill -TERM "$pid" 2>/dev/null || true
        if ! wait_gone "$pid" 50; then
            kill -KILL "$pid" 2>/dev/null || true
            killed=1
            wait_gone "$pid" 20 ||
                fail "the tray (PID $pid) does not stop. Quit it from its menu and run the installer again."
        fi
    done
}

start_tray() {
    local i
    (cd "$HOME" && exec env -u HERDCTRL_TERMINAL_COMMAND -u HERDCTRL_INSTALL_HANDOFF setsid -f "$BIN/herdctrl" --tray </dev/null >/dev/null 2>&1 9>&-) || true
    for ((i = 0; i < 30; i++)); do
        find_procs
        if ((${#trays[@]} > 0)); then return 0; fi
        sleep 0.1
    done
    warn 'The tray did not start. Start HerdCtrl from the app menu.'
}

# ---- files ---------------------------------------------------------------------------

# Replace through a temp file and mv: the running script must keep its old inode.
put_file() { # put_file <source> <destination> <mode>
    local dir tmp
    dir=$(dirname -- "$2")
    mkdir -p -- "$dir"
    tmp=$(mktemp "$dir/.herdctrl-new.XXXXXX")
    if ! { cp -- "$1" "$tmp" && chmod "$3" "$tmp" && mv -f -- "$tmp" "$2"; }; then
        rm -f -- "$tmp"
        fail "could not write $2."
    fi
}

check_bin_chars() {
    case $BIN in
        *'"'* | *'`'* | *'$'* | *'\'* | *'%'* | *$'\n'*)
            fail "the install directory $BIN contains characters a desktop entry cannot hold. Set XDG_BIN_HOME to a simpler path."
            ;;
    esac
}

write_desktop_entry() { # from the tarball's entry, with absolute Exec and TryExec
    local src=$1 out=$work/herdctrl.desktop line inserted=0
    : >"$out"
    while IFS= read -r line || [[ -n $line ]]; do
        case $line in
            Exec=* | TryExec=*) continue ;;
        esac
        printf '%s\n' "$line" >>"$out"
        if ((!inserted)) && [[ $line == '[Desktop Entry]' ]]; then
            printf 'Exec="%s/herdctrl"\nTryExec=%s/herdctrl\n' "$BIN" "$BIN" >>"$out"
            inserted=1
        fi
    done <"$src"
    ((inserted)) || fail 'the desktop entry in the archive is malformed.'
    put_file "$out" "$DATA/applications/herdctrl.desktop" 644
}

install_icons() {
    local file size
    for file in "$pkg"/desktop/hicolor/*/apps/herdctrl.png; do
        if [[ ! -f $file ]]; then continue; fi
        size=${file#"$pkg"/desktop/hicolor/}
        size=${size%%/*}
        if [[ $size =~ ^[0-9]+x[0-9]+$ ]]; then
            put_file "$file" "$DATA/icons/hicolor/$size/apps/herdctrl.png" 644
        fi
    done
}

refresh_caches() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "$DATA/applications" </dev/null >/dev/null 2>&1 || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [[ -d $DATA/icons/hicolor ]]; then
        gtk-update-icon-cache -q -t -f "$DATA/icons/hicolor" </dev/null >/dev/null 2>&1 || true
    fi
}

autostart_file() { printf '%s' "$CONFIG/autostart/herdctrl.desktop"; }

# Prints the single Exec value of the autostart entry; fails if there is not exactly one.
autostart_exec() {
    local file line count=0 value=''
    file=$(autostart_file)
    if [[ ! -f $file ]]; then return 1; fi
    while IFS= read -r line || [[ -n $line ]]; do
        if [[ $line == Exec=* ]]; then
            count=$((count + 1))
            value=${line#Exec=}
        fi
    done <"$file"
    ((count == 1)) || return 1
    printf '%s' "$value"
}

repoint_autostart() {
    local file value prog line out
    file=$(autostart_file)
    value=$(autostart_exec) || return 0
    if [[ $value != *' --tray' ]]; then return 0; fi
    prog=${value%' --tray'}
    if [[ $prog == '"'*'"' ]]; then
        prog=${prog#\"}
        prog=${prog%\"}
    elif [[ $prog == *[[:space:]]* ]]; then
        return 0 # first word is not the whole program part (env FOO=1 …): leave it alone
    fi
    if [[ ${prog##*/} != herdctrl ]]; then return 0; fi
    if [[ $value == "\"$BIN/herdctrl\" --tray" ]]; then return 0; fi
    out=$work/autostart.desktop
    : >"$out"
    while IFS= read -r line || [[ -n $line ]]; do
        if [[ $line == Exec=* ]]; then
            printf 'Exec="%s/herdctrl" --tray\n' "$BIN" >>"$out"
        else
            printf '%s\n' "$line" >>"$out"
        fi
    done <"$file"
    put_file "$out" "$file" 644
    say "Start at login now uses $BIN/herdctrl."
}

tree_hash() {
    local dir=$1 hash=none
    if [[ -d $dir ]]; then
        hash=$(cd "$dir" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum | sha256sum) || hash=none
    fi
    printf '%s' "$hash"
}

install_extension() { # install_extension <zip>
    local dir=$DATA/gnome-shell/extensions/$EXT_UUID before after shell
    if shell=$(gnome-shell --version 2>/dev/null </dev/null) && [[ ! $shell =~ GNOME\ Shell\ 50([.[:space:]]|$) ]]; then
        warn "The extension targets GNOME Shell 50; found: $shell. HerdCtrl itself still works."
    fi
    before=$(tree_hash "$dir")
    if ! gnome-extensions install --force "$1" </dev/null >/dev/null 2>&1; then
        warn "Could not install the GNOME extension. Try: gnome-extensions install --force <zip>"
        return 0
    fi
    after=$(tree_hash "$dir")
    if [[ $before == "$after" ]]; then
        say 'Extension is up to date.'
    else
        gnome-extensions enable "$EXT_UUID" </dev/null >/dev/null 2>&1 || true
        say 'Log out and back in to load the new extension.'
    fi
}

# ---- packages ------------------------------------------------------------------------

ensure_packages() {
    local package status list
    local -a missing=()
    if ((opt_no_packages)); then return 0; fi
    if ! command -v apt-get >/dev/null 2>&1 || ! command -v dpkg-query >/dev/null 2>&1; then
        say "APT not found. If HerdCtrl does not start, install: ${PACKAGES[*]}"
        return 0
    fi
    for package in "${PACKAGES[@]}"; do
        status=$(dpkg-query -W -f='${Status}' "$package" </dev/null 2>/dev/null) || status=''
        if [[ $status != 'install ok installed' ]]; then missing+=("$package"); fi
    done
    if ((${#missing[@]} == 0)); then return 0; fi
    list=${missing[*]}
    say "Installing missing packages with APT (sudo may ask for your password): $list"
    if ! sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -- "${missing[@]}" </dev/null; then
        warn "Could not install packages: $list. Install them with: sudo apt-get install $list"
    fi
}

ldd_hint() {
    local out line
    local -a libs=()
    if ! command -v ldd >/dev/null 2>&1; then return 0; fi
    out=$(ldd "$pkg/herdctrl" 2>&1 </dev/null) || out=''
    while IFS= read -r line; do
        if [[ $line == *'not found'* ]]; then libs+=("${line%% *}"); fi
    done <<<"$out"
    if ((${#libs[@]} > 0)); then
        warn "Missing libraries: ${libs[*]}. Install the packages listed in docs/manual.md."
    fi
}

# ---- archive handling ----------------------------------------------------------------

# Unpacks <tarball> into $stage; sets pkg and unpacked_version.
unpack() {
    local -a entries=()
    local entry
    stage=$work/stage
    mkdir -p -- "$stage"
    tar -xzf "$1" -C "$stage" --no-same-owner </dev/null || fail 'could not unpack the archive.'
    mapfile -d '' -t entries < <(find "$stage" -mindepth 1 -maxdepth 1 -print0)
    if ((${#entries[@]} != 1)) || [[ -L ${entries[0]} || ! -d ${entries[0]} ]]; then
        fail 'the archive must contain exactly one herdctrl-vX.Y.Z-linux-x86_64 directory.'
    fi
    entry=${entries[0]}
    if [[ ! ${entry##*/} =~ ^herdctrl-v([0-9]+\.[0-9]+\.[0-9]+)-linux-x86_64$ ]]; then
        fail 'the archive must contain exactly one herdctrl-vX.Y.Z-linux-x86_64 directory.'
    fi
    unpacked_version=${BASH_REMATCH[1]}
    pkg=$entry
    if [[ ! -f $pkg/install.sh ]]; then
        fail 'this archive predates the installer (no install.sh inside).'
    fi
    local file
    for file in herdctrl install.sh desktop/herdctrl.desktop; do
        if [[ ! -f $pkg/$file || -L $pkg/$file ]]; then fail "the archive is missing $file."; fi
    done
    if [[ ! -d $pkg/desktop/hicolor ]]; then fail 'the archive is missing desktop/hicolor.'; fi
    chmod 755 "$pkg/herdctrl"
}

smoke_test() {
    local out=''
    out=$(timeout 10 "$pkg/herdctrl" --version </dev/null 2>/dev/null) || out=''
    if [[ $out != "herdctrl $version" ]]; then
        fail "the new program does not start (expected \"herdctrl $version\"). Missing libraries? Install the packages listed in docs/manual.md."
    fi
}

# ---- commands ------------------------------------------------------------------------

do_check() {
    local latest
    latest=${opt_version:-$(latest_version)}
    read_stamp
    if [[ $installed == unknown ]]; then
        say "Installed: unknown (installed by hand or before $MIN_VERSION) · Latest: $latest → Run the installer to update."
        return 10
    fi
    if version_gt "$latest" "$installed"; then
        say "Installed: $installed · Latest: $latest → Update available. Run: herdctrl-update"
        return 10
    fi
    say "HerdCtrl $installed is up to date"
}

is_current() {
    if [[ $installed == unknown ]]; then return 1; fi
    if [[ -n $opt_version ]]; then
        [[ $installed == "$version" ]]
    else
        ! version_gt "$version" "$installed"
    fi
}

want_extension() {
    if ((opt_no_extension)); then return 1; fi
    if ((opt_extension)); then return 0; fi
    gnome_wayland
}

handoff_if_stale() {
    local own expected
    if [[ -n $handoff || ! -f $SELF ]]; then return 0; fi
    fetch install.sh
    verify_asset install.sh
    own=$(sha256sum <"$SELF") || return 0
    own=${own%% *}
    expected=$(expected_sum install.sh)
    if [[ $own == "$expected" ]]; then return 0; fi
    say "Handing over to the installer of HerdCtrl $version…"
    exec 9>&-
    local status=0
    HERDCTRL_INSTALL_HANDOFF=1 bash "$work/install.sh" "${orig_args[@]}" </dev/null || status=$?
    exit "$status"
}

do_install() {
    local tarball ext_zip='' pre_stamp
    check_bin_chars
    read_stamp
    pre_stamp=$installed
    if [[ -z $opt_archive ]]; then
        version=${opt_version:-$(latest_version)}
        if version_gt "$MIN_VERSION" "$version"; then
            say "HerdCtrl $version predates the installer; install $MIN_VERSION or newer." >&2
            exit 1
        fi
        if ((!opt_reinstall)) && is_current; then
            ensure_packages
            repoint_autostart_standalone
            if ((opt_extension)); then
                extension_only
            elif ((!opt_no_extension)) && gnome_wayland && [[ ! -d $DATA/gnome-shell/extensions/$EXT_UUID ]]; then
                say 'The GNOME extension is missing. Run: herdctrl-update --extension'
            fi
            if ((opt_extension)); then
                say "HerdCtrl $installed is already installed."
            else
                say "HerdCtrl $installed is already installed. Nothing to do."
            fi
            return 0
        fi
    else
        [[ -f $opt_archive ]] || fail "archive not found: $opt_archive"
    fi

    ensure_work
    if [[ -z $opt_archive ]]; then
        fetch SHA256SUMS
        verify_signature
        handoff_if_stale
        say "Downloading HerdCtrl $version…"
        tarball=herdctrl-v$version-linux-x86_64.tar.gz
        fetch "$tarball"
        verify_asset "$tarball"
        if want_extension && command -v gnome-extensions >/dev/null 2>&1; then
            fetch "$EXT_UUID.shell-extension.zip"
            verify_asset "$EXT_UUID.shell-extension.zip"
            ext_zip=$work/$EXT_UUID.shell-extension.zip
        fi
        unpack "$work/$tarball"
        if [[ $unpacked_version != "$version" ]]; then
            fail "the archive contains HerdCtrl $unpacked_version, expected $version. Nothing was installed."
        fi
    else
        unpack "$opt_archive"
        version=$unpacked_version
    fi

    ensure_packages
    ldd_hint
    smoke_test

    find_procs
    local restart=0 editor_open=${#editors[@]} had_tray=${#trays[@]}
    if ((${#trays[@]} > 0)); then
        if graphical_session; then
            tray_pending=1
            stop_trays
            restart=1
            if { [[ $pre_stamp == unknown ]] || ((killed)); } && gnome_wayland; then
                say "Waiting for GNOME to release the previous tray's shortcuts…"
                sleep 16
            fi
        else
            say 'The tray keeps running the previous version until you quit and restart it.'
        fi
    fi

    put_file "$pkg/herdctrl" "$BIN/herdctrl" 755
    put_file "$pkg/install.sh" "$BIN/herdctrl-update" 755
    write_desktop_entry "$pkg/desktop/herdctrl.desktop"
    install_icons
    refresh_caches
    repoint_autostart

    if want_extension; then
        if [[ -n $opt_archive ]]; then
            say 'The GNOME extension is not part of the archive; install it with scripts/install-gnome-extension.sh.'
        elif [[ -n $ext_zip ]]; then
            install_extension "$ext_zip"
        else
            warn 'gnome-extensions not found: skipping the GNOME extension.'
        fi
    fi

    printf '%s\n' "$version" >"$work/stamp"
    put_file "$work/stamp" "$DATA/herdctrl/installed-version" 644

    if ((restart)); then
        tray_pending=0
        start_tray
    elif ((had_tray == 0)); then
        say 'Start HerdCtrl from the app menu.'
    fi
    if ((editor_open > 0)); then
        say "The editor is open; close and reopen it to use $version."
    fi
    say "HerdCtrl $version is installed in $BIN."
    say 'Update later with: herdctrl-update (check with: herdctrl-update --check)'
    case ":${PATH:-}:" in
        *":$BIN:"*) ;;
        *) say "$BIN is not in your PATH yet; open a new terminal or log out and in" ;;
    esac
}

ensure_work() {
    if [[ -z $work ]]; then work=$(mktemp -d -t herdctrl-install.XXXXXXXX); fi
}

# The standalone autostart fix needs a scratch directory for its temp file.
repoint_autostart_standalone() {
    if [[ -f $(autostart_file) ]]; then
        ensure_work
        repoint_autostart
    fi
}

# --extension on an up-to-date install: verified extension only, tray untouched.
extension_only() {
    if ! command -v gnome-extensions >/dev/null 2>&1; then
        warn 'gnome-extensions not found: skipping the GNOME extension.'
        return 0
    fi
    ensure_work
    fetch SHA256SUMS
    verify_signature
    fetch "$EXT_UUID.shell-extension.zip"
    verify_asset "$EXT_UUID.shell-extension.zip"
    install_extension "$work/$EXT_UUID.shell-extension.zip"
}

do_uninstall() {
    local file value ext_dir=$DATA/gnome-shell/extensions/$EXT_UUID
    find_procs
    if ((${#trays[@]} > 0)); then stop_trays; fi

    rm -f -- "$BIN/herdctrl" "$BIN/herdctrl-update" "$DATA/applications/herdctrl.desktop" \
        "$DATA/herdctrl/installed-version"
    for file in "$DATA"/icons/hicolor/*/apps/herdctrl.png; do
        if [[ -f $file ]]; then rm -f -- "$file"; fi
    done
    rmdir -- "$DATA/herdctrl" 2>/dev/null || true
    refresh_caches

    file=$(autostart_file)
    if [[ -f $file ]]; then
        if value=$(autostart_exec) && [[ $value == "\"$BIN/herdctrl\" --tray" ]]; then
            rm -f -- "$file"
        else
            say "Left $file alone: it starts another HerdCtrl."
        fi
    fi

    if [[ -d $ext_dir ]] && command -v gnome-extensions >/dev/null 2>&1; then
        gnome-extensions uninstall "$EXT_UUID" </dev/null >/dev/null 2>&1 || warn "Could not remove the GNOME extension; remove $ext_dir by hand."
    fi

    if ((opt_purge)); then
        if [[ $CONFIG == /* && $CONFIG != / ]]; then rm -rf -- "$CONFIG/herdctrl"; fi
        say 'HerdCtrl was uninstalled, including its configuration.'
    else
        say "HerdCtrl was uninstalled. Your layouts in $CONFIG/herdctrl were kept; remove them with --purge."
    fi
}

main() {
    orig_args=("$@")
    parse_args "$@"
    if ((EUID == 0)); then fail 'Run the installer as your normal user, not with sudo.'; fi
    [[ -n ${HOME:-} ]] || fail 'HOME is not set.'
    set_paths
    if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
        fail 'HerdCtrl supports Linux x86_64 only.'
    fi
    if ((opt_check)); then
        require_tools curl
        do_check
        return
    fi
    require_tools curl tar sha256sum mktemp flock setsid timeout uname id
    acquire_lock
    if ((opt_uninstall)); then
        do_uninstall
    else
        do_install
    fi
}

main "$@"; exit
