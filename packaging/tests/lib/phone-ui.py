#!/usr/bin/env python3
"""Locate controls in an Android view hierarchy dumped by `uiautomator dump`.

The peer gates drive the OmniBridge app over adb. Fixed tap coordinates do not
survive a screen that scrolls, a peer list that grows, or a device with a
different resolution -- and a tap that lands on nothing looks exactly like a
tap that worked. Every control is therefore located by its label in the live
hierarchy, and every lookup that finds nothing exits non-zero so the caller
fails loudly instead of tapping empty space.

Subcommands
-----------
  find <regex>                     first node whose text/desc matches -> "X Y"
  find-after <anchor> <target>     first <target> BELOW the first <anchor>.
                                   This is how a control is bound to a peer:
                                   the tablet lists several desktops and each
                                   has its own "Connect" and its own chips.
  toggle-after <anchor> <label>    the checkable node nearest <label> below
                                   <anchor> -> "X Y checked"
  state <regex>                    "checked" of the nearest checkable node
  nav <label>                      the BOTTOM NAVIGATION tab with that label.
                                   Not the same as `find`: "Files" is also a
                                   per-device permission row and a capability
                                   chip, and `find` returned one of those on a
                                   sub-screen -- so the offer prompt was
                                   searched for on the screen the run was
                                   already on. A nav tab is identified by
                                   sitting on the same row as the other tabs.
  texts                            every distinct text node, one per line
"""
import re
import sys


def nodes(xml):
    out = []
    for m in re.finditer(r"<node[^>]*?/?>", xml):
        n = m.group(0)
        b = re.search(r'bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', n)
        if not b:
            continue
        x1, y1, x2, y2 = map(int, b.groups())
        t = re.search(r'text="([^"]*)"', n)
        d = re.search(r'content-desc="([^"]*)"', n)
        chk = re.search(r'checkable="(\w+)"', n)
        ck = re.search(r'checked="(\w+)"', n)
        label = (t.group(1) if t else "") or (d.group(1) if d else "")
        out.append({
            "label": label,
            "x": (x1 + x2) // 2, "y": (y1 + y2) // 2, "top": y1,
            "checkable": bool(chk and chk.group(1) == "true"),
            "checked": bool(ck and ck.group(1) == "true"),
        })
    out.sort(key=lambda n: (n["top"], n["x"]))
    return out


def match(ns, pat, after=-1):
    rx = re.compile(pat, re.I)
    return [n for n in ns if n["top"] > after and n["label"] and rx.search(n["label"])]


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    xml = open(sys.argv[1], encoding="utf-8", errors="replace").read()
    ns = nodes(xml)
    if not ns:
        print("phone-ui: the view hierarchy is empty", file=sys.stderr)
        sys.exit(2)
    cmd = sys.argv[2]

    if cmd == "texts":
        seen = set()
        for n in ns:
            if n["label"] and n["label"] not in seen:
                seen.add(n["label"])
                print(n["label"])
        return

    if cmd == "find":
        hits = match(ns, sys.argv[3])
        if not hits:
            print(f"phone-ui: no node matches {sys.argv[3]!r}", file=sys.stderr)
            sys.exit(1)
        print(hits[0]["x"], hits[0]["y"])
        return

    if cmd == "nav":
        # The bottom navigation is the row of labels lowest on the screen that
        # holds at least three of them. Requiring the whole row, rather than
        # trusting the lowest match, is what stops a sub-screen with no
        # navigation from answering with one of its own controls.
        want = re.compile(sys.argv[3], re.I)
        rows = {}
        for n in ns:
            if n["label"]:
                rows.setdefault(n["top"] // 40, []).append(n)
        best = None
        for _, group in sorted(rows.items()):
            labels = {g["label"] for g in group}
            if len(labels) >= 3 and any(want.search(lbl) for lbl in labels):
                best = group
        if best is None:
            print(f"phone-ui: no bottom-navigation row carrying {sys.argv[3]!r}",
                  file=sys.stderr)
            sys.exit(1)
        hit = next(g for g in best if want.search(g["label"]))
        print(hit["x"], hit["y"])
        return

    if cmd in ("find-after", "toggle-after", "state-after"):
        anchors = match(ns, sys.argv[3])
        if not anchors:
            print(f"phone-ui: no anchor matches {sys.argv[3]!r}", file=sys.stderr)
            sys.exit(1)
        a = anchors[0]["top"]
        hits = match(ns, sys.argv[4], after=a - 1)
        if not hits:
            print(f"phone-ui: no {sys.argv[4]!r} below {sys.argv[3]!r}", file=sys.stderr)
            sys.exit(1)
        if cmd == "find-after":
            print(hits[0]["x"], hits[0]["y"])
            return
        # The switch is a sibling of the label, on the same row.
        row = hits[0]["top"]
        sw = [n for n in ns if n["checkable"] and abs(n["top"] - row) < 90]
        if not sw:
            print(f"phone-ui: no checkable control on the row of {sys.argv[4]!r}",
                  file=sys.stderr)
            sys.exit(1)
        if cmd == "state-after":
            print("checked" if sw[0]["checked"] else "unchecked")
            return
        print(sw[0]["x"], sw[0]["y"], "checked" if sw[0]["checked"] else "unchecked")
        return

    sys.exit(__doc__)


if __name__ == "__main__":
    main()
