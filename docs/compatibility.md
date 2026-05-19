# CapyLang compatibility and integration contract

CapyLang modules must remain portable language-core logic and must run through a versioned host ABI when integrated with CapyOS.

## CapyOS reference version

- CapyOS core pinned for this contract: `0.8.0-alpha.240+20260519`
- Authoritative cross-repo matrix: `CapyOS/docs/reference/integration/compatibility-matrix.md`
- Canonical manifest format consumed by the in-tree adapter: `CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`
- Manual deploy runbook: `CapyOS/docs/operations/manual-module-deploy-runbook.md`

Authoritative CapyOS references:

- `CapyOS/docs/reference/integration/modular-installation-architecture.md`
- `CapyOS/docs/reference/integration/capylang-integration-contract.md`
- `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`
- `CapyOS/docs/reference/integration/compatibility-matrix.md`
- `CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`

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

## Publishing as a Capy package (Etapa 15, when the stage opens)

When CapyLang is delivered as a remote module to the CapyOS
`services/capypkg` adapter, the publisher must follow
`CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`.
The key requirements that affect CapyLang are:

- `payload_url` must be HTTPS only;
- `payload_sha256` must be lowercase 64 hex of the published artifact;
- `payload_size` ≤ 1 MiB during the alpha streaming-buffer window;
- `name` must follow `[a-zA-Z0-9._-]` (suggested `org.capyos.lang.runtime`);
- `install_root` must live under `/var/capypkg` or `/opt/`;
- `signature_ed25519` must cover the canonical descriptor
  `name=N|version=V|payload_sha256=H|payload_url=U\n`;
- bytecode artefacts must declare their internal magic/version inside
  the payload itself; the adapter treats payload as opaque bytes.

Until CapyAgent publishes its Ed25519 signer, CapyLang cannot be
installed from a `signed` repository in production.
