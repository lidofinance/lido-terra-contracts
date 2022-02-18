.PHONY: schema test clippy build

TOOLCHAIN := "1.55.0"

schema:
	@find contracts/* -maxdepth 0 -type d \( ! -name . \) -exec bash -c "cd '{}' && cargo +${TOOLCHAIN} schema" \;

test:
	@cargo +${TOOLCHAIN} test

clippy:
	@cargo +${TOOLCHAIN} clippy --all --all-targets -- -D warnings

build: schema clippy test
	@./build_release.sh
