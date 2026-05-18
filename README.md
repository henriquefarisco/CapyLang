# CapyLang

Version: 0.0.1

CapyLang is the external language-core repository for CapyOS.

## Status

No CapyLang implementation was found coupled inside the current CapyOS source tree. This repository starts as the authoritative external home for the language core.

## Scope owned by this repository

- Parser and syntax validation.
- Bytecode or IR format.
- VM/interpreter core.
- Minimal standard library independent from CapyOS internals.
- Host runner and mock host ABI.
- Golden tests for parser, bytecode and VM behavior.
- Deterministic benchmark programs such as `Snake` and `Asteroids` when the VM is mature enough.

## Out of scope

- Direct CapyOS syscalls.
- Kernel pointers or CapyOS internal structs.
- CapyOS filesystem, compositor, input or timer access without host ABI.
- JIT in the first integration wave.

## CapyOS integration contract

CapyOS integration must follow:

- `CapyOS/docs/reference/integration/capylang-integration-contract.md`
- `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`
- `CapyOS/docs/reference/integration/modular-installation-architecture.md`
- `docs/compatibility.md`

## Planned layout

```text
src/
  lexer/
  parser/
  bytecode/
  vm/
  stdlib/
  host_abi/
tests/
  golden/
  vm/
  host_abi/
benchmarks/
  snake/
  asteroids/
docs/
  integration.md
```

## Integration rule

CapyOS may only load CapyLang artifacts through a versioned host ABI and sandboxed bytecode loader. This repository must remain buildable and testable without CapyOS kernel headers.
