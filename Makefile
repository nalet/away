# Away, built with one command: make
#
#   make          the program, in target/release/away
#   make test     every test of the program
#   make check    what the pipeline checks on every change, short of a cluster
#   make image    the image, as away:dev (needs Docker)
#   make clean    remove what was built

COMMIT := $(shell git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)

.PHONY: all build test check image clean

all: build

build:
	AWAY_COMMIT=$(COMMIT) cargo build --release --locked

test:
	cargo test --locked

check:
	python3 scripts/model.py --check
	python3 scripts/libraries.py --check
	cargo fmt --all --check
	cargo clippy --all-targets --locked -- -D warnings
	helm lint chart

image:
	docker build --build-arg AWAY_COMMIT=$(COMMIT) --tag away:dev .

clean:
	cargo clean
