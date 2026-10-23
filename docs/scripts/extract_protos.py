#!/usr/bin/env python3
"""Recover .proto schemas from jadx-decompiled Java (full protobuf, not lite).

protoc embeds each file's serialized FileDescriptorProto as Java string literals passed to
Descriptors.FileDescriptor.internalBuildGeneratedFileFrom(new String[]{...}, deps). Each char is
one byte (ISO-8859-1), so unescaping the literals and parsing the bytes gives the exact schema.

usage: extract_protos.py <jadx sources dir> <out dir>
"""
import pathlib
import re
import sys

from google.protobuf import descriptor_pb2

CALL = "internalBuildGeneratedFileFrom(new String[]{"
LITERAL = re.compile(r'\s*"((?:[^"\\]|\\.)*)"\s*[,+]?', re.S)
ESC = {"n": "\n", "t": "\t", "b": "\b", "r": "\r", "f": "\f", '"': '"', "'": "'", "\\": "\\"}

TYPES = {v: k[5:].lower() for k, v in descriptor_pb2.FieldDescriptorProto.Type.items()}
LABELS = {1: "optional", 2: "required", 3: "repeated"}


def unescape(s):
    out, i = [], 0
    while i < len(s):
        c = s[i]
        if c != "\\":
            out.append(c)
            i += 1
        elif s[i + 1] == "u":
            out.append(chr(int(s[i + 2:i + 6], 16)))
            i += 6
        elif s[i + 1] in "01234567":  # octal escape, up to 3 digits
            m = re.match(r"[0-7]{1,3}", s[i + 1:])
            out.append(chr(int(m.group(0), 8)))
            i += 1 + len(m.group(0))
        else:
            out.append(ESC[s[i + 1]])
            i += 2
    return "".join(out)


def varint(n):
    out = bytearray()
    while True:
        out.append((n & 0x7F) | (0x80 if n > 0x7F else 0))
        n >>= 7
        if not n:
            return bytes(out)


def repair_name_length(raw):
    """Fix field 1 (`name`, tag 0x0a) whose length prefix disagrees with the `.proto` name it holds.

    knote2's NoteShapeDocProto/ReaderShapeDocProto ship with prefixes 3 short (20/22 for 23/25
    chars): the literal was renamed "...ShapeProto" -> "...ShapeDocProto" without fixing the length.
    Shape-based: the name is the bytes after the prefix up to the first ".proto".
    Returns (fixed bytes, note) or (None, None) when the shape does not match.
    """
    if len(raw) < 3 or raw[0] != 0x0A or raw[1] & 0x80:
        return None, None
    end = raw.find(b".proto", 2, 2 + 0x7F)
    if end < 0:
        return None, None
    actual = end + len(".proto") - 2
    if actual == raw[1]:
        return None, None
    return raw[:1] + varint(actual) + raw[2:], f"name length prefix repaired (stored {raw[1]}, actual {actual})"


def parse(raw):
    """FileDescriptorProto from bytes; retries once with a repaired name prefix. -> (fd, note)"""
    fd = descriptor_pb2.FileDescriptorProto()
    try:
        fd.ParseFromString(raw)
        return fd, None
    except Exception:
        fixed, note = repair_name_length(raw)
        if fixed is None:
            raise
        fd.ParseFromString(fixed)  # still failing → the caller reports the error
        return fd, note


def descriptors(java_text):
    # Walk literal by literal: a regex up to "}," would stop at a "}," inside the bytes.
    start = java_text.find(CALL)
    while start >= 0:
        pos, parts = start + len(CALL), []
        while (m := LITERAL.match(java_text, pos)):
            parts.append(unescape(m.group(1)))
            pos = m.end()
        if not java_text[pos:].lstrip().startswith("}"):
            sys.exit(f"[protos] FAILED: unexpected token after literal {len(parts)}: {java_text[pos:pos + 40]!r}")
        raw = "".join(parts)
        start = java_text.find(CALL, pos)
        try:
            fd, note = parse(raw.encode("latin-1"))
        except Exception as err:  # some literals don't round-trip; report, don't abort the batch
            yield None, f"{type(err).__name__} on {len(raw)} bytes (first literal {raw[:40]!r})", None
            continue
        yield fd, None, note


def field_type(f):
    if f.type in (f.TYPE_MESSAGE, f.TYPE_ENUM):
        return f.type_name.lstrip(".")
    return TYPES[f.type]


def render_message(m, indent, syntax3):
    pad = "  " * indent
    lines = [f"{pad}message {m.name} {{"]
    for e in m.enum_type:
        lines += render_enum(e, indent + 1)
    for n in m.nested_type:
        if n.options.map_entry:
            continue
        lines += render_message(n, indent + 1, syntax3)
    maps = {n.name: n for n in m.nested_type if n.options.map_entry}
    for f in m.field:
        entry = maps.get(f.type_name.rsplit(".", 1)[-1]) if f.label == 3 else None
        if entry:
            k, v = entry.field
            decl = f"map<{field_type(k)}, {field_type(v)}>"
        else:
            label = "" if syntax3 and f.label == 1 else LABELS[f.label] + " "
            decl = label + field_type(f)
        lines.append(f"{pad}  {decl} {f.name} = {f.number};")
    lines.append(f"{pad}}}")
    return lines


def render_enum(e, indent):
    pad = "  " * indent
    return [f"{pad}enum {e.name} {{"] + [f"{pad}  {v.name} = {v.number};" for v in e.value] + [f"{pad}}}"]


def render(fd, note=None):
    syntax3 = fd.syntax == "proto3"
    lines = [f"// Recovered from the embedded FileDescriptorProto by docs/scripts/extract_protos.py",
             *([f"// Note: {note}"] if note else []),
             f'syntax = "{fd.syntax or "proto2"}";']
    if fd.package:
        lines.append(f"package {fd.package};")
    lines += [f'import "{d}";' for d in fd.dependency]
    for k in ("java_package", "java_outer_classname"):
        if fd.options.HasField(k):
            lines.append(f'option {k} = "{getattr(fd.options, k)}";')
    lines.append("")
    for e in fd.enum_type:
        lines += render_enum(e, 0) + [""]
    for m in fd.message_type:
        lines += render_message(m, 0, syntax3) + [""]
    return "\n".join(lines)


def main():
    src, out = map(pathlib.Path, sys.argv[1:3])
    out.mkdir(parents=True, exist_ok=True)
    n = bad = 0
    for java in sorted(src.rglob("*.java")):
        text = java.read_text(errors="replace")
        if "internalBuildGeneratedFileFrom" not in text:
            continue
        for fd, err, note in descriptors(text):
            if err:
                print(f"[protos] SKIPPED {java.relative_to(src)}: {err}")
                bad += 1
                continue
            (out / pathlib.Path(fd.name).name).write_text(render(fd, note))
            print(f"[protos] {java.relative_to(src)} -> {fd.name}: "
                  f"{len(fd.message_type)} messages, {len(fd.enum_type)} enums" + (f" ({note})" if note else ""))
            n += 1
    print(f"[protos] {n} files written to {out}, {bad} descriptors skipped")
    if n == 0:
        sys.exit("[protos] FAILED: no embedded descriptors found")


if __name__ == "__main__":
    main()
