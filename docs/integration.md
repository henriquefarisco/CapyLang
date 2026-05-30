# CapyLang integration reference

## Boundary

CapyLang core is developed outside CapyOS. CapyOS integrates it only through bytecode artifacts and a versioned host ABI.

## Required artifacts

- bytecode magic/version;
- target host ABI version;
- checksum;
- declared permissions;
- optional debug metadata stripped for release.

## Host ABI modules

Initial host ABI modules should match the CapyOS contract:

- `time`;
- `log`;
- `fs` sandbox;
- `config`;
- `gfx2d`;
- `input`;
- `metrics`.

## Security rules

- no direct syscalls;
- no host pointers in bytecode;
- no JIT for the first CapyOS integration;
- deterministic error returns;
- instruction/time budget per frame or command;
- all host resources represented by opaque handles.

## Reference adapter (host-test only)

S7 ships an in-process reference `HostAdapter` inside the `capy-vm`
crate (`HostAdapter`, `HostFn`, `HostResult`) so the host bridge is
exercisable from Rust unit tests without standing up CapyOS. The
reference adapter exposes deterministic stubs for `time::now` and
`log::info` only and is **not** part of the CapyOS integration
contract: the real adapter is owned by CapyOS and binds against
`(module, symbol)` pairs declared in the bytecode `Imports` section.
Handler signatures preserve the boundary: handlers see a borrowed
slice of opaque `Value`s and return a single `Value`; failure reasons
must be `&'static str` so dynamic content cannot leak into
diagnostics. The reference adapter remains review/edit only on this
machine (see local execution policy).

## CapyOS stage

Official integration belongs to CapyOS Etapa 15. Development in this repository does not count as CapyOS roadmap progress until the CapyOS adapter and external gates are added.

## Cross-repo references

- `CapyOS/docs/reference/integration/compatibility-matrix.md`
- `CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`
- `CapyOS/docs/operations/manual-module-deploy-runbook.md`
- `CapyOS/docs/architecture/capypkg-adapter.md`

CapyOS core pinned for this contract: `0.8.0-alpha.261+20260529`.
