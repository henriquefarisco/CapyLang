.PHONY: all clean lint security test validate version-check \
        build test-rust fmt-check clippy rust-validate update-goldens \
        package package-clean

# Default target keeps the legacy doc/policy validation path that CI
# (.github/workflows/ci.yml) already depends on. Rust validation lives in
# the dedicated `rust-validate` target and runs on its own CI workflow
# (.github/workflows/rust.yml).
all: validate

# === Doc and policy gates ===================================================

lint:
	git -c core.whitespace=cr-at-eol diff --check
	@v=$$(cat VERSION); test -n "$$v" || (echo "VERSION is empty" >&2; exit 1)
	! grep -R "$$(printf '\t')" README.md docs

security:
	grep -R "no direct syscalls" docs README.md
	grep -R "sandboxed bytecode loader" README.md docs
	grep -R "instruction/time budget" docs README.md

test:
	test -s README.md
	test -s docs/compatibility.md
	test -s docs/integration.md
	test -s docs/grammar.ebnf
	test -s docs/bytecode-v0.md

version-check:
	@v=$$(cat VERSION); \
	grep -q "Version: $$v" README.md || (echo "README.md missing Version: $$v" >&2; exit 1)

validate: lint security test version-check

# === Rust (capy-lexer and future crates) ====================================

build:
	cargo build --workspace --all-targets

test-rust:
	cargo test --workspace --all-targets
	cargo test --workspace --doc

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

rust-validate: fmt-check clippy test-rust

update-goldens:
	CAPY_GOLDEN_UPDATE=1 cargo test --workspace --all-targets

# === capypkg packaging (Etapa 9 alpha) ======================================
#
# `make package` produces the artefact + line-oriented manifest the
# CapyOS in-tree `services/capypkg` adapter consumes. The payload
# tarball is intentionally a snapshot of the repository sources
# (crates + docs + VERSION) — bytecode artefacts go through their
# own publishing flow once the Rust pipeline matures.

CAPY_PKG_NAME := org.capyos.lang.runtime
CAPY_PKG_VERSION := $(shell cat VERSION)
CAPY_PKG_SUMMARY := CapyLang Rust lexer + future VM (host-testable snapshot)
CAPY_PKG_INSTALL_ROOT := /var/capypkg/$(CAPY_PKG_NAME)
CAPY_PKG_PROVIDES_ABI := capy-lang-host
CAPY_PKG_ABI_VERSION := 0
CAPY_PKG_CORE_ABI_MIN := 3
CAPY_PKG_CORE_ABI_MAX := 3
CAPY_PKG_KNOWN_GOOD := 0
CAPY_PKG_DEPENDS :=
PUBLISH_URL_BASE ?= https://github.com/henriquefarisco/CapyLang/releases/download/v$(CAPY_PKG_VERSION)
CAPY_PKG_BUILD_DIR := target/capypkg
CAPY_PKG_BIN := $(CAPY_PKG_BUILD_DIR)/$(CAPY_PKG_NAME)-$(CAPY_PKG_VERSION).bin
CAPY_PKG_MANIFEST := $(CAPY_PKG_BUILD_DIR)/$(CAPY_PKG_NAME).manifest

package: $(CAPY_PKG_MANIFEST)

$(CAPY_PKG_BIN):
	@mkdir -p $(CAPY_PKG_BUILD_DIR)
	@tar --format=ustar --owner=0 --group=0 --numeric-owner \
	     --mtime='@0' --sort=name \
	     -cf $@ crates docs VERSION 2>/dev/null || \
	  tar -cf $@ crates docs VERSION
	@echo "[package] $@"

$(CAPY_PKG_MANIFEST): $(CAPY_PKG_BIN)
	@SHA=$$(shasum -a 256 $(CAPY_PKG_BIN) 2>/dev/null | awk '{print $$1}' | tr 'A-F' 'a-f') ; \
	if [ -z "$$SHA" ]; then SHA=$$(sha256sum $(CAPY_PKG_BIN) | awk '{print $$1}' | tr 'A-F' 'a-f'); fi ; \
	SIZE=$$(wc -c < $(CAPY_PKG_BIN) | tr -d ' ') ; \
	URL="$(PUBLISH_URL_BASE)/$(CAPY_PKG_NAME)-$(CAPY_PKG_VERSION).bin" ; \
	{ \
	  echo "name=$(CAPY_PKG_NAME)" ; \
	  echo "version=$(CAPY_PKG_VERSION)" ; \
	  echo "summary=$(CAPY_PKG_SUMMARY)" ; \
	  echo "payload_url=$$URL" ; \
	  echo "payload_sha256=$$SHA" ; \
	  echo "payload_size=$$SIZE" ; \
	  echo "install_root=$(CAPY_PKG_INSTALL_ROOT)" ; \
	  echo "provides_abi=$(CAPY_PKG_PROVIDES_ABI)" ; \
	  echo "abi_version=$(CAPY_PKG_ABI_VERSION)" ; \
	  echo "core_abi_min=$(CAPY_PKG_CORE_ABI_MIN)" ; \
	  echo "core_abi_max=$(CAPY_PKG_CORE_ABI_MAX)" ; \
	  echo "known_good=$(CAPY_PKG_KNOWN_GOOD)" ; \
	  echo "depends=$(CAPY_PKG_DEPENDS)" ; \
	  echo "---" ; \
	} > $@
	@echo "[package] manifest: $@"

package-clean:
	rm -rf $(CAPY_PKG_BUILD_DIR)

clean:
	cargo clean 2>/dev/null || true
	rm -rf $(CAPY_PKG_BUILD_DIR)
