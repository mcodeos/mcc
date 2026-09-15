#!/bin/sh
# Copyright (c) 2026 MCode
#
# Licensed under either of Apache License, Version 2.0 or MIT License at your option.
#
# Build-slot picker. Cargo holds `<target-dir>/<profile>/.cargo-lock` for the
# whole of a build, so every session sharing one target dir queues behind the
# others. A slot is a private target dir per session.
#
#   slot a -> target-slots/a/
#   slot b -> target-slots/b/
#   slot c -> target-slots/c/
#
# Slot 0 -> `target/0/` is **not claimable**: it belongs to the IDE, whose
# rust-analyzer would otherwise hold a session's slot for the whole of a
# `cargo check` and block that session's build. Point rust-analyzer at it with
# `rust-analyzer.cargo.targetDir` (see `.vscode/settings.json`) and reach it
# from a shell with `MCC_SLOT=0`. Because a claim never lands on 0, the IDE
# never has to queue behind a session and a session never queues behind the IDE.
#
# The session slots live in `target-slots/`, **outside** slot 0's `target/`:
# nested roots share a name space, so a `cargo clean` (or any stray file) at the
# outer root would reach every slot inside it. Disjoint roots make each slot's
# `cargo clean` reach only that slot.
#
# `mcc` on PATH and anything else that hardcoded `target/debug/mcc` must move to
# the slot's own path (`target-slots/a/debug/mcc`) or to `$MCC_BIN`. A
# compatibility symlink at `target/debug` would serve both, but it cannot exist
# yet: an editor window that has not picked up `rust-analyzer.cargo.targetDir`
# still checks in `target/debug`, and a symlink there would route those writes
# back into slot `a`.
#
# The slot is pinned per session, so every command in one session keeps using
# the same target dir and the same binary. The pin lives in `target-slots/.pins/`
# keyed by the session identity, and records the top-most ancestor PID of the
# session so a pin left behind by a dead session is reclaimed on the next claim.
#
# Modes, chosen by the name this script is invoked as:
#
#   scripts/mcc-slot.sh              print shell exports for the pinned slot
#   eval "$(scripts/mcc-slot.sh)"    -> CARGO_TARGET_DIR, MCC_BIN, MCC_SLOT
#
#   mcc ...                          (symlinked as `mcc` on PATH) exec the
#                                    pinned slot's binary with the given args.
#                                    A bare `mcc` claims no slot: it uses the
#                                    session's pin if there is one, else the
#                                    first slot that holds a binary.
#
# Set MCC_SLOT=a, MCC_SLOT=b, MCC_SLOT=c or MCC_SLOT=0 to force a slot.

set -eu

# `$0` may be a symlink on PATH, so follow it before deriving the repo root;
# `readlink -f` is not portable, hence the explicit loop.
script=$0
while [ -L "$script" ]; do
    here=$(cd "$(dirname "$script")" && pwd)
    script=$(readlink "$script")
    case "$script" in
        /*) ;;
        *) script="$here/$script" ;;
    esac
done
root=$(cd "$(dirname "$script")/.." && pwd)
slots="a b c"
# Slot 0 is known but never claimed: it is the IDE's, not a session's.
known_slots="0 $slots"
pin_dir="$root/target-slots/.pins"

slot_dir() {
    case "$1" in
        0) printf '%s/target/0' "$root" ;;
        a) printf '%s/target-slots/a' "$root" ;;
        b) printf '%s/target-slots/b' "$root" ;;
        c) printf '%s/target-slots/c' "$root" ;;
    esac
}

top_ancestor() {
    pid=$$
    while :; do
        ppid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')
        [ -n "$ppid" ] || break
        [ "$ppid" -gt 1 ] 2>/dev/null || break
        pid=$ppid
    done
    printf '%s' "$pid"
}

# Identity of the session, not of the shell: an agent session issues each
# command in a fresh shell, so a PID-based key would move between commands.
session_key() {
    key=${ICUBE_CODEMAIN_SESSION:-}
    [ -n "$key" ] || key=${TRAE_SANDBOX_SBOX_ID:-}
    [ -n "$key" ] || key="ancestor-$(top_ancestor)"
    printf '%s' "$key" | tr -c 'A-Za-z0-9._-' '-' | cut -c1-64
}

# A slot is busy while a live process holds its cargo lock.
slot_busy() {
    for lock in "$(slot_dir "$1")"/*/.cargo-lock; do
        [ -f "$lock" ] || continue
        if lsof "$lock" >/dev/null 2>&1; then
            return 0
        fi
    done
    return 1
}

# A slot that already holds a binary has a warm dependency tree, so taking it
# spares the session a full rebuild from cold.
slot_warm() {
    [ -x "$(slot_dir "$1")/debug/mcc" ]
}

# `$RANDOM` is a bash extension and `sort -R` a GNU one, so randomize in awk.
# The seed mixes the shell PID with the clock: `srand()` alone would repeat
# within the same second and make every quick invocation pick the same slot.
pick_random() {
    printf '%s\n' "$1" | tr ' ' '\n' | sed '/^$/d' | awk -v seed="$$$(date +%s)" '
        BEGIN { srand(seed) }
        { line[NR] = $0 }
        END { if (NR > 0) print line[int(rand() * NR) + 1] }
    '
}

# Claim a slot no other live session holds. Preference, in order: idle and warm
# (reuses an existing dependency tree), idle, then anything — queueing beats
# refusing to run.
claim_slot() {
    held=""
    for pin in "$pin_dir"/*; do
        [ -f "$pin" ] || continue
        [ "$(basename "$pin")" = "$key" ] && continue
        owner=$(cut -d' ' -f2 < "$pin")
        if [ -n "$owner" ] && kill -0 "$owner" 2>/dev/null; then
            held="$held $(cut -d' ' -f1 < "$pin")"
        else
            rm -f "$pin"
        fi
    done

    candidates=""
    for s in $slots; do
        case " $held " in *" $s "*) continue ;; esac
        candidates="$candidates $s"
    done
    [ -n "$candidates" ] || candidates=" $slots"

    warm=""
    idle=""
    for s in $candidates; do
        slot_busy "$s" && continue
        idle="$idle $s"
        slot_warm "$s" && warm="$warm $s"
    done

    for tier in "$warm" "$idle" "$candidates"; do
        if [ -n "$tier" ]; then
            pick_random "$tier"
            return 0
        fi
    done
}

mkdir -p "$pin_dir"
key=$(session_key)
pin="$pin_dir/$key"
mode=${0##*/}

# An explicit MCC_SLOT is a one-off override; it leaves the session's pin alone.
slot=${MCC_SLOT:-}
if [ -z "$slot" ]; then
    if [ -f "$pin" ]; then
        slot=$(cut -d' ' -f1 < "$pin" | tr -d '[:space:]')
    fi
    case " $slots " in
        *" $slot "*) ;;
        *)
            if [ "$mode" = mcc ]; then
                # A bare `mcc` is not a session and must not claim one: every
                # terminal that ran it would take a slot and never give it
                # back. Run the first slot that has a binary, else `a`.
                slot=a
                for s in $slots; do
                    slot_warm "$s" && { slot=$s; break; }
                done
            else
                slot=$(claim_slot)
            fi
            ;;
    esac
fi

# Only a session records a pin; a bare `mcc` reads one at most.
if [ "$mode" != mcc ] || [ -f "$pin" ]; then
    printf '%s %s\n' "$slot" "$(top_ancestor)" > "$pin"
fi
case " $known_slots " in
    *" $slot "*) ;;
    *)
        printf 'mcc-slot: unknown slot "%s" (use %s)\n' "$slot" "$known_slots" >&2
        exit 2
        ;;
esac

bin=$(slot_dir "$slot")/debug/mcc
case "${0##*/}" in
    mcc)
        if [ ! -x "$bin" ] && [ -x "$root/target-slots/a/debug/mcc" ]; then
            # A slot with no build yet must not break `mcc`; fall back to
            # slot a's binary rather than failing.
            printf 'mcc: slot %s has no binary yet, using %s\n' \
                "$slot" "$root/target-slots/a/debug/mcc" >&2
            bin=$root/target-slots/a/debug/mcc
        fi
        if [ ! -x "$bin" ]; then
            printf 'mcc: no binary in build slot %s (%s)\n' "$slot" "$bin" >&2
            printf 'mcc: build it with:  cd %s && cargo build\n' "$root" >&2
            exit 127
        fi
        exec "$bin" "$@"
        ;;
esac

printf 'export MCC_SLOT=%s\n' "$slot"
printf 'export CARGO_TARGET_DIR=%s\n' "$(slot_dir "$slot")"
printf 'export MCC_BIN=%s\n' "$bin"
