# Away, built with one command: make
#
#   make          the pages, then the program with them inside, in target/release/away
#   make pages    the pages alone, in web/dist
#   make test     every test of the program
#   make test-pages   the pages, gone through in a browser (once: npx --prefix web playwright install chromium)
#   make check    what the pipeline checks on every change, short of a cluster
#   make image    the image, as away:dev (needs Docker)
#   make clean    remove what was built

COMMIT := $(shell git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)

.PHONY: all pages build test test-pages check image clean

all: build

pages:
	npm --prefix web ci --no-audit --no-fund
	npm --prefix web run build

build: pages
	AWAY_COMMIT=$(COMMIT) cargo build --release --locked

test: pages
	cargo test --locked

test-pages: pages
	cargo build --locked --package away-server --example pages --features stand-in
	npm --prefix web test

check: pages
	python3 scripts/model.py --check
	python3 scripts/libraries.py --check
	cargo fmt --all --check
	cargo clippy --all-targets --locked -- -D warnings
	helm lint chart

image:
	docker build --build-arg AWAY_COMMIT=$(COMMIT) --tag away:dev .

clean:
	cargo clean
	rm -rf web/dist web/node_modules
