# CapyLang bytecode v0 (specification placeholder)

Status: pinned by slice S4 in the CapyLang roadmap. This document reserves
the file path and fixes the **header** layout so external tooling and the
CapyOS integration adapter (Etapa 15) can already validate magic, version
and ABI fields. Opcodes, constant pool layout and verifier rules will be
populated during S4 itself.

Authoritative roadmap reference:
`.windsurf/plans/capylang-roadmap-0c1ca5.md`

## Versioning

Bytecode versioning is independent from the language version. The container
declares its own integer version inside the header.

- v0  — initial stack-based encoding (this document)
- v1  — optimised layout with debug info in a separate segment (slice S23)
- v2+ — SSA-friendly encoding suitable for JIT (Fase 3, M2/M3)

Loaders MUST accept the current major plus the immediately preceding major
through a translation shim. Loaders MUST reject unknown majors with a
deterministic error code.

## Header (frozen by S4)

The first 32 bytes of every CapyLang bytecode artifact follow this layout.
All multi-byte integers are little-endian.

| Offset | Size | Field          | Notes                                  |
|-------:|-----:|----------------|----------------------------------------|
|      0 |    4 | `magic`        | ASCII `CAPY` (0x43 0x41 0x50 0x59)     |
|      4 |    2 | `bc_version`   | bytecode container version (v0 -> 0)   |
|      6 |    2 | `abi_version`  | required host ABI version              |
|      8 |    4 | `flags`        | reserved, must be zero in v0           |
|     12 |    4 | `body_length`  | bytes of body following the header     |
|     16 |   16 | `checksum`     | BLAKE3-128 of the body                 |

## Body (drafted by S4)

The body is divided into sections introduced by a 1-byte tag and a 4-byte
length. The set of tags is fixed at v0 and additive within v0.x.

| Tag  | Section            | Purpose                                  |
|-----:|--------------------|------------------------------------------|
| 0x01 | `consts`           | constant pool (ints, floats, strings)    |
| 0x02 | `functions`        | function table (entry, locals, code)     |
| 0x03 | `imports`          | host ABI imports declared by the module  |
| 0x04 | `debug` (optional) | source spans and symbol names            |

Concrete encoding rules for each section are defined in slice S4.

## Security invariants (cannot regress)

The following constraints are duplicated here from
`docs/integration.md` so loaders can be audited against this document:

- no direct syscalls;
- no host pointers in bytecode;
- no JIT for the first CapyOS integration;
- deterministic error returns;
- instruction/time budget per frame or command;
- all host resources represented by opaque handles.

## Open questions (resolved by S4)

- final opcode numbering and ranges reserved for additive growth;
- exact integer/float encoding inside the constant pool;
- debug section layout (line table vs. compressed spans);
- versioning of the `debug` tag relative to the container.
