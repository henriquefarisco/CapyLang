# CapyLang bytecode v0

Status: delivered by slices S4 (header), S4b (typed body sections),
S5a (instruction set) and S5c (static stack-balance verifier), all
implemented in the `capy-bytecode` crate. The **header** layout is frozen;
external tooling and the CapyOS integration adapter (Etapa 15) can validate
magic, version and ABI fields against this document. Opcodes, the constant
pool layout and the verifier rules are specified below.

Authoritative roadmap reference: `docs/roadmap.md`

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

## Body (framed by S4, typed by S4b)

The body is divided into sections introduced by a 1-byte tag and a 4-byte
length. The set of tags is fixed at v0 and additive within v0.x.

| Tag  | Section            | Purpose                                  |
|-----:|--------------------|------------------------------------------|
| 0x01 | `consts`           | constant pool (ints, floats, strings)    |
| 0x02 | `functions`        | function table (entry, locals, code)     |
| 0x03 | `imports`          | host ABI imports declared by the module  |
| 0x04 | `debug` (optional) | source spans and symbol names            |

### `consts` (0x01)

```text
u32 count
count * (u8 const_tag + payload)
```

| Const tag | Variant       | Payload                                 |
|----------:|---------------|-----------------------------------------|
|      0x01 | `Int(i64)`    | 8-byte little-endian signed integer     |
|      0x02 | `Float(f64)`  | 8-byte little-endian IEEE-754 binary64  |
|      0x03 | `Str(String)` | u32 byte length + UTF-8 bytes           |

Adding a new const tag is additive within v0; renaming or reusing an
existing tag is breaking.

### `functions` (0x02)

```text
u32 count
count * (
    u32 name_len  + name_bytes  (UTF-8)
    u32 locals_count
    u32 code_len  + code_bytes  (opaque opcodes)
)
```

`code_bytes` contains a stream of v0 instructions (frozen by S5a). The
per-function framing is independent of the instruction set, but the
instruction set itself is part of the `capy-lang-host` v0 contract: a
loader/disassembler may decode any function's `code_bytes` according to
the *Instruction set* table below.

#### Instruction set (frozen by S5a)

Every instruction is one opcode byte followed by an optional immediate.
Immediates fall into three shapes:

- `U32`    — 4-byte little-endian unsigned (used for indices).
- `I32`    — 4-byte little-endian signed (used for relative jump offsets).
- `U32U32` — two consecutive 4-byte little-endian unsigned values; used
  by `call` to carry `(fn_idx, argc)` and by `host_call` to carry
  `(import_idx, argc)`.

For `Jump` / `JumpIfFalse` the offset is computed from the byte that
follows the full instruction (PC after decoding the immediate); offset
`0` therefore means "fall through".

| Byte | Mnemonic         | Immediate | Stack effect                            |
|-----:|------------------|-----------|-----------------------------------------|
| 0x00 | `nop`            | —         | unchanged                               |
| 0x01 | `pop`            | —         | `a -> `                                 |
| 0x10 | `load_const`     | `U32`     | ` -> const[i]`                          |
| 0x11 | `load_true`      | —         | ` -> true`                              |
| 0x12 | `load_false`     | —         | ` -> false`                             |
| 0x13 | `load_none`      | —         | ` -> ()`                                |
| 0x20 | `load_local`     | `U32`     | ` -> locals[i]`                         |
| 0x21 | `store_local`    | `U32`     | `a -> ` (writes locals[i])              |
| 0x30 | `add`            | —         | `a b -> a+b`                            |
| 0x31 | `sub`            | —         | `a b -> a-b`                            |
| 0x32 | `mul`            | —         | `a b -> a*b`                            |
| 0x33 | `div`            | —         | `a b -> a/b`                            |
| 0x34 | `mod`            | —         | `a b -> a%b`                            |
| 0x35 | `neg`            | —         | `a -> -a`                               |
| 0x36 | `band`           | —         | `a b -> a&b` (bitwise, ints)            |
| 0x37 | `bor`            | —         | `a b -> a\|b` (bitwise, ints)           |
| 0x38 | `bxor`           | —         | `a b -> a^b` (bitwise, ints)            |
| 0x39 | `shl`            | —         | `a b -> a<<b` (ints, count mod 64)      |
| 0x3A | `shr`            | —         | `a b -> a>>b` (ints, arithmetic)        |
| 0x40 | `eq`             | —         | `a b -> a==b`                           |
| 0x41 | `ne`             | —         | `a b -> a!=b`                           |
| 0x42 | `lt`             | —         | `a b -> a<b`                            |
| 0x43 | `le`             | —         | `a b -> a<=b`                           |
| 0x44 | `gt`             | —         | `a b -> a>b`                            |
| 0x45 | `ge`             | —         | `a b -> a>=b`                           |
| 0x50 | `not`            | —         | `a -> !a`                               |
| 0x51 | `bnot`           | —         | `a -> ~a` (bitwise, ints)               |
| 0x60 | `make_array`     | `U32`     | `v0..vN-1 -> [v0, ..., vN-1]` (n elems) |
| 0x61 | `array_get`      | —         | `arr idx -> arr[idx]` (bounds-checked)  |
| 0x62 | `array_set`      | —         | `arr idx val -> arr` (writes in place)  |
| 0x63 | `array_len`      | —         | `arr -> len(arr)` (Int)                 |
| 0x70 | `jump`           | `I32`     | unchanged (PC += imm)                   |
| 0x71 | `jump_if_false`  | `I32`     | `a -> ` (jumps if `!a`)                 |
| 0x80 | `call`           | `U32U32`  | `arg0..argN-1 -> ret` (transfer to fn)  |
| 0x81 | `return`         | —         | terminates the function                 |
| 0x82 | `host_call`      | `U32U32`  | `arg0..argN-1 -> ret` (dispatch to host)|

`call (fn_idx, argc)` pops the top `argc` values into the callee's
`locals[0..argc]` (slot `0` is the first argument, in source order) and
transfers control to the start of `functions[fn_idx]`. The callee's
`return` pops its top-of-stack value and pushes it onto the caller's
operand stack, resuming at the byte immediately after the `call`
instruction. Recursion is allowed; the host (VM) enforces a deterministic
call-depth limit and traps fail-closed when exceeded.

`host_call (import_idx, argc)` pops the top `argc` values, looks up
`imports[import_idx]` in the module's `Imports` section to obtain a
`(module, symbol)` pair, dispatches through the VM's registered
[`HostAdapter`] and pushes the returned value back onto the operand
stack. The host handler never observes raw VM internals; arguments
cross the boundary as a borrowed slice of opaque [`Value`]s and the
return is a single [`Value`]. Unresolved imports, arity mismatches and
handler-reported errors trap deterministically (`V0014`-`V0016`); JIT
and direct syscalls remain forbidden.

Unused byte values are reserved for additive growth within v0. Adding a
new opcode is additive; renaming or reusing an existing byte value is
breaking. `&&` / `||` short-circuit lowering (S5b.2) uses only the
existing `jump` / `jump_if_false` opcodes. `call` (0x80) was introduced
by S5b.3; `host_call` (0x82) by S7; the integer bitwise / shift opcodes
`band` / `bor` / `bxor` / `shl` / `shr` (0x36-0x3A) and `bnot` (0x51)
were added by S5d and operate on integer operands only (a non-integer
operand traps with `V0005 TYPE_MISMATCH`; the shift count is reduced
modulo 64). The aggregate opcodes `make_array` / `array_get` /
`array_set` / `array_len` (0x60-0x63) were added by S6.2 (see
`docs/aggregates.md`) and operate on arrays: a non-array or non-int
operand traps with `V0005 TYPE_MISMATCH`, and an out-of-range or negative
index traps with `V0017 INDEX_OUT_OF_BOUNDS`. The rest of the `0x60-0x6F`
block stays reserved for future aggregate ops (tuple / struct in S6.3).
`match` (S2.2b) parses today but its lowering will introduce additional
opcodes that append to this table in a future slice.

### `imports` (0x03)

```text
u32 count
count * (
    u32 module_len + module_bytes (UTF-8)
    u32 symbol_len + symbol_bytes (UTF-8)
)
```

Each entry declares one host ABI symbol the module imports
(e.g. `time::now`, `log::info`). The CapyOS host adapter binds these
against its versioned import table at load time.

### `debug` (0x04, optional)

```text
u32 count
count * (
    u32 bytecode_offset    (byte index inside the function code)
    u32 source_start       (byte offset in the original source)
    u32 source_end         (byte offset in the original source, ≥ source_start)
)
```

Future S4 sub-slices may append fields after `source_end` per entry; the
schema is therefore reserved at the entry boundary, not at the section
boundary.

### Error codes

Loader errors carry a stable `&'static str` code. The catalogue is
additive within v0; renaming or removing a code is a breaking change.

| Code  | Variant                  | Description                              |
|------:|--------------------------|------------------------------------------|
| B0001 | `MagicMismatch`          | bytes 0..4 ≠ `CAPY`                      |
| B0002 | `UnsupportedBcVersion`   | `bc_version` > loader maximum            |
| B0003 | `ReservedFlagsNonZero`   | `flags` ≠ 0 in v0                        |
| B0004 | `BodyLengthMismatch`     | declared body length ≠ trailing bytes    |
| B0005 | `ChecksumMismatch`       | BLAKE3-128 of body differs from header   |
| B0006 | `MalformedSection`       | unknown tag, truncated header, overflow  |
| B0007 | `TruncatedHeader`        | input shorter than 32 bytes              |
| B0008 | `MalformedConstants`     | invalid const pool payload               |
| B0009 | `MalformedFunctions`     | invalid function table payload           |
| B0010 | `MalformedImports`       | invalid import table payload             |
| B0011 | `MalformedDebug`         | invalid debug info payload               |
| B0012 | `MalformedInstruction`   | unknown opcode or truncated immediate    |

### Verifier error codes (S5c)

The static stack-balance verifier (`capy-bytecode`'s `verify_function`,
run by the VM at load time via `Vm::from_module`) rejects any function
whose operand-stack discipline cannot be proven. Its failures use the same
`B<NNNN>` family but are surfaced through the VM as
`VmError::MalformedModule` carrying the verifier's stable code, rather than
by the container loader above. The catalogue is additive within v0.

| Code  | Constant                            | Description                                          |
|------:|-------------------------------------|------------------------------------------------------|
| B0013 | `B_VERIFIER_STACK_UNDERFLOW`        | an opcode pops more operands than the stack provides |
| B0014 | `B_VERIFIER_STACK_INCONSISTENCY`    | two predecessors disagree on operand-stack depth     |
| B0015 | `B_VERIFIER_FALL_OFF_END`           | a reachable path ends without a terminating `return` |
| B0016 | `B_VERIFIER_INVALID_RETURN_DEPTH`   | `return` reached with operand-stack depth not exactly 1 |
| B0017 | `B_VERIFIER_LOCAL_OUT_OF_BOUNDS`    | `load_local` / `store_local` slot ≥ `locals_count`   |
| B0018 | `B_VERIFIER_JUMP_OUT_OF_BOUNDS`     | `jump` / `jump_if_false` target off an instruction boundary |
| B0019 | `B_VERIFIER_UNKNOWN_FUNCTION_INDEX` | `call` references an out-of-range function index     |
| B0020 | `B_VERIFIER_CALL_ARITY_OVERFLOW`    | `call` `argc` exceeds the callee's `locals_count`    |

## Security invariants (cannot regress)

The following constraints are duplicated here from
`docs/integration.md` so loaders can be audited against this document:

- no direct syscalls;
- no host pointers in bytecode;
- no JIT for the first CapyOS integration;
- deterministic error returns;
- instruction/time budget per frame or command;
- all host resources represented by opaque handles.

## Resolved by S4 / S4b / S5a

The questions originally tracked here are now specified above:

- final opcode numbering and ranges reserved for additive growth — see the
  *Instruction set* table (S5a);
- exact integer/float encoding inside the constant pool — see `consts`
  (S4b: `Int` i64 LE, `Float` f64 IEEE-754 LE, `Str` u32 length + UTF-8);
- debug section layout — see `debug` (S4b: per-entry `bytecode_offset`,
  `source_start`, `source_end`); a per-entry function-index field is
  reserved for v1;
- versioning of the `debug` tag relative to the container — the `debug`
  section is optional within v0 and additive; v1 may extend each entry.
