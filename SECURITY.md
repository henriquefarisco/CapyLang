# Security Policy

CapyLang 0.0.1 is an early service release. Report security issues privately to the repository owner before opening public issues.

## Release gate

- `make validate` must pass before release tags.
- CapyOS integration must remain bytecode-only through a sandboxed host ABI.
- Documentation must keep the no-direct-syscalls, no-host-pointers and budgeted-execution constraints visible.
