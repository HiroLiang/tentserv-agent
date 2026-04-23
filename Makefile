.PHONY: help fmt check build run-cli run-http clean

help:
	@printf "%s\n" \
	"make fmt       Format Rust workspace" \
	"make check     Check Rust workspace" \
	"make build     Build Rust workspace" \
	"make run-cli   Run CLI package (pass ARGS='...')" \
	"make run-http  Run HTTP package" \
	"make clean     Clean Rust build artifacts"

fmt:
	cargo fmt --all

check:
	cargo check --workspace

build:
	cargo build --workspace

run-cli:
	TENTGENT_HOME="$${TENTGENT_HOME:-$$PWD/.tentgent}" cargo run -p tentgent-cli -- $(ARGS)

run-http:
	TENTGENT_HOME="$${TENTGENT_HOME:-$$PWD/.tentgent}" cargo run -p tentgent-http -- $(ARGS)

clean:
	cargo clean
