.PHONY: all clean lint security test validate version-check

all: validate

lint:
	git diff --check
	test "$$(cat VERSION)" = "0.0.1"
	! grep -R "$$(printf '\t')" README.md docs

security:
	grep -R "no direct syscalls" docs README.md
	grep -R "sandboxed bytecode loader" README.md docs
	grep -R "instruction/time budget" docs README.md

test:
	test -s README.md
	test -s docs/compatibility.md
	test -s docs/integration.md

version-check:
	test "$$(cat VERSION)" = "0.0.1"
	grep -q "Version: 0.0.1" README.md

validate: lint security test version-check

clean:
	true
