#!/usr/bin/env python3
"""Pull every SecRule out of a CRS tree and say what each one needs (T504).

SecLang is line-oriented with backslash continuations and three
double-quoted fields, which is little enough grammar to read directly. The
point is not a parser — it is a census: which operators CRS actually uses,
how often, and what each @rx pattern would demand of a regex engine.
"""
import json
import re
import sys
from pathlib import Path

DIRECTIVE = re.compile(r'^\s*(SecRule|SecAction|SecMarker|SecDefaultAction|SecRuleRemoveById|'
                       r'SecRuleUpdateTargetById|SecRuleUpdateActionById|SecComponentSignature|'
                       r'SecCollectionTimeout|SecArgumentSeparator|SecRequestBodyAccess|'
                       r'SecResponseBodyAccess|SecRuleEngine|SecRuleUpdateTargetByTag)\b')


def join_continuations(text):
    """Yields (first_line_no, logical_line)."""
    out, buf, start = [], None, None
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.rstrip("\n")
        stripped = line.strip()
        if buf is None:
            if not stripped or stripped.startswith("#"):
                continue
            buf, start = line, n
        else:
            buf = buf[:-1] + " " + stripped if buf.rstrip().endswith("\\") else buf + " " + stripped
        if buf.rstrip().endswith("\\"):
            buf = buf.rstrip()[:-1]
            continue
        out.append((start, " ".join(buf.split())))
        buf, start = None, None
    if buf is not None:
        out.append((start, " ".join(buf.split())))
    return out


def split_fields(line):
    """SecRule VARS "OP" "ACTIONS" — fields may be quoted or bare."""
    fields, i, n = [], 0, len(line)
    while i < n:
        while i < n and line[i] == " ":
            i += 1
        if i >= n:
            break
        if line[i] == '"':
            i += 1
            start = i
            buf = []
            while i < n:
                if line[i] == "\\" and i + 1 < n:
                    # SecLang only unescapes the quote itself inside a quoted
                    # field; CRS writes a literal backslash as \x5c precisely so
                    # that everything else can pass through to the regex engine
                    # untouched.
                    if line[i + 1] == '"':
                        buf.append('"')
                    else:
                        buf.append(line[i:i + 2])
                    i += 2
                    continue
                if line[i] == '"':
                    break
                buf.append(line[i])
                i += 1
            fields.append("".join(buf))
            i += 1
        else:
            start = i
            while i < n and line[i] != " ":
                i += 1
            fields.append(line[start:i])
    return fields


OPERATOR = re.compile(r'^\s*(!?)@(\w+)\s*(.*)$', re.S)


def parse_operator(raw):
    m = OPERATOR.match(raw)
    if m:
        return m.group(2), m.group(3)
    # No explicit operator means @rx in SecLang.
    return "rx", raw


def parse_actions(raw):
    """id, phase, paranoia level and transformations out of the action list."""
    out = {"id": None, "phase": None, "pl": None, "t": [], "tags": [], "chain": False}
    depth, buf, parts = 0, [], []
    for ch in raw:
        if ch == "'":
            depth ^= 1
        if ch == "," and not depth:
            parts.append("".join(buf))
            buf = []
        else:
            buf.append(ch)
    parts.append("".join(buf))
    for part in parts:
        part = part.strip()
        if part == "chain":
            out["chain"] = True
        if ":" not in part:
            continue
        key, _, value = part.partition(":")
        key, value = key.strip(), value.strip().strip("'")
        if key == "id":
            out["id"] = value
        elif key == "phase":
            out["phase"] = value
        elif key == "t":
            out["t"].append(value)
        elif key == "tag":
            out["tags"].append(value)
            if value.startswith("paranoia-level/"):
                out["pl"] = int(value.split("/")[1])
    return out


def main(root):
    root = Path(root)
    files = sorted(root.glob("rules/*.conf")) + sorted(root.glob("*.conf.example"))
    rules, directives = [], {}
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        for lineno, line in join_continuations(text):
            m = DIRECTIVE.match(line)
            if not m:
                continue
            name = m.group(1)
            directives[name] = directives.get(name, 0) + 1
            if name not in ("SecRule", "SecAction"):
                continue
            fields = split_fields(line)
            if name == "SecAction":
                continue
            if len(fields) < 3:
                continue
            _, variables, operator_raw = fields[0], fields[1], fields[2]
            actions_raw = fields[3] if len(fields) > 3 else ""
            op, arg = parse_operator(operator_raw)
            actions = parse_actions(actions_raw)
            rules.append({
                "file": path.name,
                "line": lineno,
                "id": actions["id"],
                "vars": variables,
                "op": op,
                "arg": arg,
                "pl": actions["pl"],
                "phase": actions["phase"],
                "t": actions["t"],
                "chain": actions["chain"],
            })
    json.dump({"rules": rules, "directives": directives}, sys.stdout)


if __name__ == "__main__":
    main(sys.argv[1])
