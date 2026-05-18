# CapyLang compatibility and integration contract

CapyLang modules must remain portable language-core logic and must run through a versioned host ABI when integrated with CapyOS.

Authoritative CapyOS references:

- `CapyOS/docs/reference/integration/modular-installation-architecture.md`
- `CapyOS/docs/reference/integration/capylang-integration-contract.md`
- `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`

## Owned ABI

CapyLang shares ownership of the `capy-lang-host` ABI with CapyOS.

This ABI covers:

- bytecode/IR loading boundary;
- host calls exposed to programs;
- sandboxable VM execution;
- resource limits;
- deterministic VM errors;
- benchmark program contracts when applicable.

## Compatibility rules

- Bytecode or IR formats must be explicitly versioned.
- Host ABI calls must be additive until the integration stage permits migration.
- Programs must not call CapyOS syscalls or use kernel pointers directly.
- Filesystem, network, UI, input and timers must be accessed only through host ABI grants.
- JIT is out of scope for the first integration wave.

## Install/update boundary

CapyLang runtime artifacts may be optional components. CapyOS owns:

- sandbox policy;
- process/resource limits;
- filesystem/network grants;
- UI/input/timer host functions;
- staging, activation and rollback.

## Dependency rules

CapyLang components may depend on:

- `capy-lang-host`;
- `capy-benchmark-report` for benchmark workloads;
- CapyOS host ABI components explicitly listed by the package descriptor.

They must not depend on kernel headers or runtime internals.

## Validation before CapyOS integration

Before CapyOS consumes a CapyLang release, externally validate:

- parser fixtures;
- bytecode/IR compatibility fixtures;
- VM deterministic execution;
- host ABI mock tests;
- sandbox/resource-limit behavior;
- benchmark determinism when benchmark programs are included.

CapyLang integration is gated by Etapa 15.
