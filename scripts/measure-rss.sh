#!/usr/bin/env bash
# Measure resident memory of herdctrl one second after its window appears,
# and the time from start until that window is mapped.
#
# Usage: scripts/measure-rss.sh [runs] [binary]
#   runs    measured starts (default 5); one extra warm-up start is excluded.
#   binary  executable to start (default target/release/herdctrl).
#
# Each start uses a throwaway XDG_CONFIG_HOME, so the user's layouts are not
# touched. Only the PID started by this script is ever signalled.
set -u

runs="${1:-5}"
binary="${2:-target/release/herdctrl}"

if [[ ! -x "$binary" ]]; then
    echo "error: executable not found: $binary (run cargo build --release --locked)" >&2
    exit 1
fi

have_x=0
if command -v xwininfo >/dev/null 2>&1; then
    have_x=1
fi

# Waits up to 5 s for a HerdCtrl X11 window owned by PID $1, so other open
# HerdCtrl windows do not count. Falls back to a fixed 1.5 s delay without xwininfo.
wait_for_window() {
    if [[ $have_x -eq 0 ]]; then
        sleep 1.5
        return 1
    fi
    local i id
    for i in $(seq 1 500); do
        for id in $(xwininfo -root -tree 2>/dev/null | awk '/"HerdCtrl"/ { print $1 }'); do
            if xprop -id "$id" _NET_WM_PID 2>/dev/null | grep -q "= $1\$"; then
                return 0
            fi
        done
        sleep 0.01
    done
    return 1
}

# Prints "VmRSS RssAnon RssFile RssShmem" in KiB for a PID.
read_rss() {
    awk '
        /^VmRSS:/    { rss = $2 }
        /^RssAnon:/  { anon = $2 }
        /^RssFile:/  { file = $2 }
        /^RssShmem:/ { shmem = $2 }
        END { printf "%s %s %s %s\n", rss, anon, file, shmem }
    ' "/proc/$1/status"
}

# Median of one numeric column of stdin.
median() {
    sort -n | awk '{ a[NR] = $1 } END {
        if (NR == 0) { print 0; exit }
        if (NR % 2) print a[(NR + 1) / 2]; else print (a[NR / 2] + a[NR / 2 + 1]) / 2
    }'
}

config_dir="$(mktemp -d)"
trap 'rm -rf "$config_dir"' EXIT

results_file="$config_dir/results"
total=$((runs + 1))
for ((run = 1; run <= total; run++)); do
    started=$(date +%s%N)
    XDG_CONFIG_HOME="$config_dir" "$binary" >/dev/null 2>&1 &
    pid=$!
    visible="n/a"
    if wait_for_window "$pid"; then
        visible=$(( ($(date +%s%N) - started) / 1000000 ))
    fi
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "error: herdctrl exited early on run $run" >&2
        exit 1
    fi
    sample="$(read_rss "$pid")"
    # The editor starts a tray child for this throwaway configuration; stop it too.
    tray="$(pgrep -P "$pid" -f -- '--tray' || true)"
    kill "$pid" $tray 2>/dev/null
    wait "$pid" 2>/dev/null
    if [[ $run -eq 1 ]]; then
        echo "warm-up (excluded): VmRSS/RssAnon/RssFile/RssShmem KiB = $sample, visible after $visible ms"
        continue
    fi
    echo "run $((run - 1)): VmRSS/RssAnon/RssFile/RssShmem KiB = $sample, visible after $visible ms"
    echo "$sample $visible" >> "$results_file"
done

echo "median over $runs runs (KiB): " \
    "VmRSS=$(awk '{print $1}' "$results_file" | median)" \
    "RssAnon=$(awk '{print $2}' "$results_file" | median)" \
    "RssFile=$(awk '{print $3}' "$results_file" | median)" \
    "RssShmem=$(awk '{print $4}' "$results_file" | median)" \
    "visible_ms=$(awk '$5 != "n/a" {print $5}' "$results_file" | median)"
