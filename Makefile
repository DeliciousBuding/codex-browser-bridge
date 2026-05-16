.PHONY: build test install-local clean

VERSION ?= dev

build:
	go build -trimpath -ldflags "-s -w -X main.version=$(VERSION)" -o bin/codex-browser-bridge ./cmd/bridge

test:
	go vet ./...
	go test ./...

install-local: build
	cp bin/codex-browser-bridge ~/.local/bin/codex-browser-bridge

clean:
	rm -rf bin/
