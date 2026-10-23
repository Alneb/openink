#!/usr/bin/env python3
"""Onyx SDK device layer → framework bindings (docs/API-COVERAGE.md).

For every public method of SDMDevice (knote2's bundled SDK, jadx output), list the framework methods it
invokes by reflection: `FIELD = ReflectUtil.get[Declared]MethodSafely(classVar, "name", argTypes…)` binds
a field; a method body that passes FIELD (or a local alias of it) to `ReflectUtil.invokeMethodSafely`
calls it. Shape-based: class variables, arg-type variables and aliases are resolved, not hardcoded.
Usage: sdk_bindings.py <SDMDevice.java> [out.tsv]   (read the output through docs/scripts/safecat.sh)
"""
import re
import sys
from pathlib import Path

CLASS_VAR = re.compile(r"Class<\?>\s+(\w+)\s*=\s*ReflectUtil\.classForName\(\"([\w.$]+)\"\)")
TYPE_VAR = re.compile(r"(?:Class<\?>|Class)\s+(\w+)\s*=\s*([\w.]+)\.(TYPE|class)\s*;")
BIND = re.compile(r"(\w+)\s*=\s*ReflectUtil\.get(?:Declared)?MethodSafely\((\w+|[\w.]+\.class),\s*\"(\w+)\"\s*(?:,\s*(.*?))?\);")
METHOD = re.compile(r"^    public (?:static )?(?:synchronized )?([\w<>\[\], .?]+?) (\w+)\(([^)]*)\)\s*(?:throws [\w, .]+)?\s*\{", re.M)
ALIAS = re.compile(r"Method\s+(\w+)\s*=\s*(\w+)\s*;")
INVOKE = re.compile(r"ReflectUtil\.invokeMethodSafely\((\w+)\s*,\s*([^,)]+)")


def body_at(src: str, start: int) -> str:
    depth, i = 0, src.index("{", start)
    for j in range(i, len(src)):
        depth += {"{": 1, "}": -1}.get(src[j], 0)
        if depth == 0:
            return src[i:j + 1]
    return src[i:]


def type_name(expr: str, types: dict[str, str]) -> str:
    expr = expr.strip()
    if expr in types:
        return types[expr]
    if expr == "new Class[0]":
        return ""
    return re.sub(r"\.(class|TYPE)$", "", expr).split(".")[-1]


def main():
    src = Path(sys.argv[1]).read_text(errors="replace")
    classes = {v: c for v, c in CLASS_VAR.findall(src)}
    types = {v: (t if kind == "class" else {"Integer": "int", "Boolean": "boolean", "Float": "float", "Long": "long"}.get(t, t)).split(".")[-1]
             for v, t, kind in TYPE_VAR.findall(src)}
    binds = {}
    for field, cvar, name, args in BIND.findall(src):
        cls = classes.get(cvar, re.sub(r"\.class$", "", cvar))
        argl = [type_name(a, types) for a in re.split(r",\s*(?![^()]*\))", args)] if args else []
        argl = [a for a in argl if a]
        binds[field] = f"{cls.split('.')[-1]}.{name}({', '.join(argl)})"
    rows = []
    for m in METHOD.finditer(src):
        ret, name, params = m.group(1), m.group(2), m.group(3)
        body = body_at(src, m.end() - 1)
        alias = dict(ALIAS.findall(body))
        targets = []
        for f, recv in INVOKE.findall(body):
            b = binds.get(alias.get(f, f))
            if b:
                kind = "static" if recv.strip() == "null" else "on " + recv.strip()
                targets.append(f"{b} [{kind}]")
        ptypes = ", ".join(p.strip().rsplit(" ", 1)[0] for p in params.split(",") if p.strip())
        rows.append((name, ptypes, ret.strip(), " ; ".join(dict.fromkeys(targets)) or "-"))
    out = "\n".join("\t".join(r) for r in rows) + "\n"
    if len(sys.argv) > 2:
        Path(sys.argv[2]).write_text(out)
    sys.stdout.write(out)
    print(f"[sdk-bindings] {len(binds)} bindings, {len(rows)} public methods, "
          f"{sum(r[3] != '-' for r in rows)} reach the framework", file=sys.stderr)


if __name__ == "__main__":
    main()
