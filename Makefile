.PHONY: build build-rust test test-rust install-local clean

VERSION ?= dev

build:
	go build -trimpath -ldflags "-s -w -X main.version=$(VERSION)" -o bin/codex-browser-bridge.exe ./cmd/bridge

build-rust:
	cargo build --locked --release

test:
	go vet ./...
	go test ./...

test-rust:
	cargo check --locked
	cargo test --locked

install-local: build
	cp bin/codex-browser-bridge.exe ~/.local/bin/codex-browser-bridge.exe

clean:
	rm -rf bin/
