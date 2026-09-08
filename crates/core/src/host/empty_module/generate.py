#!/usr/bin/env python3
"""Reproduce the version-1 system empty module using only Python's standard library.

The binary has one page of memory, a V10 schema describer, and a reducer entry
point which always traps. There are no declared user tables or functions.

Equivalent code (the data payload below is BSATN, not WebAssembly encoding):
  (import "spacetime_10.0" "bytes_sink_write" (func (param i32 i32 i32) (result i32)))
  (import "spacetime_10.6" "get_call_auth_flags" (func (result i32)))
  (memory (export "memory") 1 1)
  (func (export "__describe_module__") (param i32)
    local.get 0 i32.const 32 i32.const 16 call 0
    if unreachable end)
  (func (export "__call_reducer__")
    (param i32 i64 i64 i64 i64 i64 i64 i64 i32 i32) (result i32)
    unreachable)

The unused 10.6 import explicitly requires the host ABI which supports captured
invocation flags. No function or invocation context exists in this module.
"""

import argparse
import hashlib
from pathlib import Path
import struct


def leb(value):
    encoded = bytearray()
    while value >= 128:
        encoded.append((value & 127) | 128)
        value >>= 7
    encoded.append(value)
    return bytes(encoded)


def string(value):
    value = value.encode("utf-8")
    return leb(len(value)) + value


def vector(items):
    return leb(len(items)) + b"".join(items)


def section(tag, payload):
    return bytes([tag]) + leb(len(payload)) + payload


def function_type(params, results):
    return b"\x60" + vector(params) + vector(results)


def generate():
    u32 = lambda value: struct.pack("<I", value)
    # RawModuleDef::V10 (sum tag 2), two V10 sections:
    # Typespace (tag 0, empty vector) and Capabilities (tag 13, one RawIdentifier).
    capability = b"hosted_auth_v1"
    schema = b"\x02" + u32(2) + b"\x00" + u32(0) + b"\x0d" + u32(1) + u32(len(capability)) + capability
    i32, i64 = b"\x7f", b"\x7e"
    types = vector([
        function_type([i32, i32, i32], [i32]),
        function_type([], [i32]),
        function_type([i32], []),
        function_type([i32] + [i64] * 7 + [i32, i32], [i32]),
    ])
    imports = vector([
        string("spacetime_10.0") + string("bytes_sink_write") + b"\x00" + leb(0),
        string("spacetime_10.6") + string("get_call_auth_flags") + b"\x00" + leb(1),
    ])
    exports = vector([
        string("memory") + b"\x02" + leb(0),
        string("__describe_module__") + b"\x00" + leb(2),
        string("__call_reducer__") + b"\x00" + leb(3),
    ])
    describe = b"\x00\x20\x00\x41\x20\x41\x10\x10\x00\x04\x40\x00\x0b\x0b"
    reducer = b"\x00\x00\x0b"
    code = vector([leb(len(body)) + body for body in [describe, reducer]])
    data = vector([
        b"\x00\x41\x10\x0b" + leb(4) + u32(len(schema)),
        b"\x00\x41\x20\x0b" + leb(len(schema)) + schema,
    ])
    wasm = b"\x00asm\x01\x00\x00\x00" + b"".join([
        section(1, types), section(2, imports), section(3, vector([leb(2), leb(3)])),
        section(5, vector([b"\x01\x01\x01"])), section(7, exports),
        section(10, code), section(11, data),
    ])
    return wasm, schema


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify checked-in files without modifying them")
    args = parser.parse_args()
    wasm, schema = generate()
    files = {
        "v1.wasm": wasm,
        "v1.schema.bsatn": schema,
        "v1.sha256": (hashlib.sha256(wasm).hexdigest() + "  v1.wasm\n").encode(),
    }
    root = Path(__file__).resolve().parent
    for name, contents in files.items():
        path = root / name
        if args.check:
            if not path.is_file() or path.read_bytes() != contents:
                raise SystemExit(f"{name} differs from the deterministic generator")
        else:
            path.write_bytes(contents)
    print(f"version 1: {len(wasm)} Wasm bytes, {len(schema)} BSATN bytes; reproducible files verified")


if __name__ == "__main__":
    main()
