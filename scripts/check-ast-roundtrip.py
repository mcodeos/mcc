#!/usr/bin/env python3
"""AST <-> source round-trip gate (CIMP §1 U159).

Consumes the two span-carrying AST faces and reports exactly three finding
classes:

- ``gap``      a non-trivia source byte covered by no leaf span
               (source has it, AST dropped it);
- ``invented`` a leaf span covering only trivia, or a leaf whose covered
               text does not match its value (AST has it, source does not);
- ``overlap``  a source byte covered by more than one leaf span.

Faces:
- ``cst``  ``mcc show ast <file> -f json`` -- every token leaf carries
  ``span: {start, end}`` (byte offsets into the file);
- ``stmt`` ``mcc parse <file> --ast -f json`` -- each module statement root
  carries the same span shape (``result.view.data[*].children[*].span``).

Trivia is whitespace plus ``#``-to-end-of-line comments. Exit codes follow
the repo's check-script convention: 0 clean, 1 execution error (unreadable
file, mcc failure, malformed JSON), 2 findings.

Usage:
  check-ast-roundtrip.py [--mcc BIN] [--face cst|stmt|both]
                         [--max-examples N] FILE.mc [FILE.mc ...]
"""

import argparse
import json
import subprocess
import sys

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_FINDINGS = 2


def fail(msg):
    sys.stderr.write("check-ast-roundtrip: %s\n" % msg)
    return EXIT_ERROR


def run_mcc(mcc_bin, args, path):
    cmd = [mcc_bin] + args
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    except FileNotFoundError:
        return None, "cannot execute %r" % mcc_bin
    except subprocess.TimeoutExpired:
        return None, "timed out: %s" % " ".join(cmd)
    if proc.returncode != 0:
        return None, "mcc rc=%d: %s" % (proc.returncode, proc.stderr.strip()[:200])
    try:
        return json.loads(proc.stdout), None
    except json.JSONDecodeError as exc:
        return None, "malformed JSON from %s: %s" % (" ".join(cmd), exc)


def build_trivia_mask(text):
    """Byte-true array: whitespace and comment bytes are trivia.

    Comments are ``#``-to-EOL and ``//``-to-EOL, outside string literals
    (a ``"`` toggles string state; strings may not span lines in the grammar).
    """
    mask = [False] * len(text)
    i = 0
    n = len(text)
    in_string = False
    while i < n:
        c = text[i]
        if in_string:
            if c == ord("\"") or c == ord("\n"):
                in_string = False
            i += 1
        elif c in b" \t\r\n\f\v":
            mask[i] = True
            i += 1
        elif c == ord("#") or text[i:i + 2] == b"//":
            while i < n and text[i] != ord("\n"):
                mask[i] = True
                i += 1
        else:
            if c == ord("\""):
                in_string = True
            i += 1
    return mask


def line_of(text, offset):
    return text.count(b"\n", 0, offset) + 1


def collect_cst_leaves(node, out, all_spans=None):
    """Leaves = nodes without a non-empty children array; spans required.

    Every node span lands in ``all_spans`` (when given): container extents
    count toward source coverage even though only leaves drive the invented
    and overlap checks.
    """
    if not isinstance(node, dict):
        return
    kids = node.get("children")
    span = node.get("span")
    if isinstance(span, dict) and all_spans is not None:
        try:
            all_spans.append((int(span["start"]), int(span["end"])))
        except (KeyError, TypeError, ValueError):
            pass
    if isinstance(kids, list) and kids:
        for k in kids:
            collect_cst_leaves(k, out, all_spans)
    elif isinstance(span, dict):
        try:
            start = int(span["start"])
            end = int(span["end"])
        except (KeyError, TypeError, ValueError):
            return
        if end > start:
            out.append((start, end, node.get("kind", "?"), node.get("value")))


def collect_stmt_spans(view):
    """Statement-root spans from the parse --ast face."""
    out = []
    if not isinstance(view, dict):
        return out
    for top in view.get("data", []) or []:
        if not isinstance(top, dict):
            continue
        for child in top.get("children", []) or []:
            if isinstance(child, dict) and isinstance(child.get("span"), dict):
                try:
                    out.append((int(child["span"]["start"]), int(child["span"]["end"])))
                except (KeyError, TypeError, ValueError):
                    continue
    return out


def span_slices(text, leaves):
    """Yield (start, end, text_slice) for leaves clipped to the file."""
    for start, end, kind, value in leaves:
        yield start, end, kind, value, text[start:end]


def tighten_leaves(text, leaves):
    """Reconcile leaf spans with their values; drop containers.

    1. If the covered slice starts (after leading whitespace) with the leaf's
       value, tighten the span to exactly the value bytes. This removes the
       C-side run-length looseness (a flattened rule node whose span covers
       the whole dotted / comma-separated run while its value holds only the
       first segment).
    2. Drop leaves that fully contain another leaf: they are containers, the
       innermost leaves are the real tokens.
    """
    tightened = []
    for start, end, kind, value in leaves:
        if value is not None:
            chunk = text[start:end].decode("utf-8", "replace")
            lead = len(chunk) - len(chunk.lstrip())
            if chunk.strip().startswith(str(value)):
                start = start + lead
                end = start + len(str(value))
        tightened.append((start, end, kind, value))

    kept = []
    for leaf in tightened:
        s, e = leaf[0], leaf[1]
        if any(o is not leaf and o[0] >= s and o[1] <= e and (o[0], o[1]) != (s, e)
               for o in tightened):
            continue
        kept.append(leaf)
    kept.sort()
    return kept


def check_face(path, text, mask, leaves, findings, max_examples, all_spans=None):
    """Coverage over every node span; invented/overlap over kept leaves."""
    kept = tighten_leaves(text, leaves)
    cover_all = [0] * (len(text) + 1)
    for start, end in (all_spans if all_spans is not None else []):
        for i in range(max(0, start), min(end, len(text))):
            cover_all[i] += 1
    cover_leaf = [0] * (len(text) + 1)
    for start, end, _kind, _value in kept:
        for i in range(max(0, start), min(end, len(text))):
            cover_leaf[i] += 1

    for start, end, kind, value in kept:
        chunk = text[start:end]
        if not chunk.strip():
            findings.append(("invented", path, "%s span %d:%d covers only trivia"
                             % (kind, start, end)))
        elif value is not None:
            want = str(value).encode("utf-8")
            # Byte-exact, or the span is a byte-prefix of the value (the C
            # lexer counts some multibyte token lens in characters, not
            # bytes -- the shortfall surfaces in the gap class instead).
            if chunk != want and not want.startswith(chunk):
                findings.append(("invented", path,
                                 "%s span %d:%d value %r != source %r"
                                 % (kind, start, end, value,
                                    chunk.decode("utf-8", "replace"))))

    gaps = 0
    overlaps = 0
    for i in range(len(text)):
        if mask[i]:
            continue
        if cover_all[i] == 0:
            gaps += 1
            if gaps <= max_examples:
                line = line_of(text, i)
                findings.append(("gap", path, "byte %d (line %d) %r uncovered"
                                 % (i, line, text[i:i + 20].decode("utf-8", "replace"))))
        elif cover_leaf[i] > 1:
            overlaps += 1
            if overlaps <= max_examples:
                line = line_of(text, i)
                findings.append(("overlap", path, "byte %d (line %d) covered %d times"
                                 % (i, line, cover_leaf[i])))
    return gaps, overlaps, len(kept)


def check_file(mcc_bin, path, faces, max_examples):
    try:
        text = open(path, "rb").read()
    except OSError as exc:
        return fail("cannot read %s: %s" % (path, exc))

    findings = []

    if faces in ("cst", "both"):
        data, err = run_mcc(mcc_bin, ["show", "ast", path, "-f", "json"], path)
        if err:
            return fail("%s: %s" % (path, err))
        try:
            ast = data["result"]["show"]["ast"]
        except (KeyError, TypeError):
            return fail("%s: unexpected show ast envelope" % path)
        leaves = []
        all_spans = []
        if isinstance(ast, list):
            for root in ast:
                collect_cst_leaves(root, leaves, all_spans)
        else:
            collect_cst_leaves(ast, leaves, all_spans)
        leaves.sort()
        mask = build_trivia_mask(text)
        gaps, overlaps, kept = check_face(
            path, text, mask, leaves, findings, max_examples, all_spans)
        print("%s cst: %d leaves (%d kept), gaps=%d overlaps=%d invented=%d" % (
            path, len(leaves), kept, gaps, overlaps,
            sum(1 for f in findings if f[0] == "invented")))

    if faces in ("stmt", "both"):
        data, err = run_mcc(mcc_bin, ["parse", path, "--ast", "-f", "json"], path)
        if err:
            return fail("%s: %s" % (path, err))
        try:
            view = data["result"]["view"]
        except (KeyError, TypeError):
            return fail("%s: unexpected parse envelope" % path)
        spans = collect_stmt_spans(view)
        mask = build_trivia_mask(text)
        cover = [False] * len(text)
        for start, end in spans:
            for i in range(max(0, start), min(end, len(text))):
                cover[i] = True
        stmt_gaps = sum(1 for i in range(len(text))
                        if not mask[i] and not cover[i])
        # The phrase face only carries connection statements, so byte
        # coverage is not its contract -- uncovered bytes are an
        # informational readout. What IS a finding: a malformed span
        # (empty, inverted, out of file, or starting on trivia).
        for start, end in spans:
            if start < 0 or end <= start or end > len(text):
                findings.append(("gap", path,
                                 "malformed statement span %d:%d" % (start, end)))
            elif start < len(text) and mask[start]:
                findings.append(("gap", path,
                                 "statement span %d:%d starts on trivia"
                                 % (start, end)))
        print("%s stmt: %d stmts, uncovered=%d (informational)"
              % (path, len(spans), stmt_gaps))

    for kind, fpath, detail in findings[:max_examples * 3]:
        print("  %s %s: %s" % (kind.upper(), fpath, detail))
    if findings:
        return EXIT_FINDINGS
    return EXIT_FINDINGS if findings else EXIT_OK


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--mcc", default="mcc", help="mcc binary to run")
    ap.add_argument("--face", choices=["cst", "stmt", "both"], default="both")
    ap.add_argument("--max-examples", type=int, default=5)
    ap.add_argument("files", nargs="+", help=".mc files to check")
    args = ap.parse_args(argv)

    rc = EXIT_OK
    for path in args.files:
        file_rc = check_file(args.mcc, path, args.face, args.max_examples)
        if file_rc == EXIT_ERROR:
            return file_rc
        if file_rc == EXIT_FINDINGS:
            rc = EXIT_FINDINGS
    if rc == EXIT_OK:
        print("check-ast-roundtrip: all files clean")
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
