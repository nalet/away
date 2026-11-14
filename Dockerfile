# syntax=docker/dockerfile:1
#
# Away's image: the one program, and nothing else.
#
# The first stage builds the program for the image's processor, linked so that it needs
# no library when it runs. The second holds that one file. It has no shell and no package
# manager, and it runs as an ordinary user.
#
# The compiler here is the one rust-toolchain.toml names; `make check` fails when the two
# differ.
FROM rust:1.99-alpine@sha256:44819b27be4c97b0e4d7203bd2e02c4820f97700211b6a37fb4c246a2de59383 AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# the commit the image is built from, which the program then names as its own
ARG AWAY_COMMIT=unknown
ENV AWAY_COMMIT=${AWAY_COMMIT}
RUN cargo build --release --locked --bin away \
 && cp target/release/away /away

FROM scratch
ARG AWAY_COMMIT=unknown
LABEL org.opencontainers.image.title="away" \
      org.opencontainers.image.description="Away, remote access in a browser: the one program that runs as its server, its gateway and its issuer." \
      org.opencontainers.image.source="https://github.com/nalet/away" \
      org.opencontainers.image.licenses="MPL-2.0" \
      org.opencontainers.image.revision="${AWAY_COMMIT}"
COPY --from=build /away /away
USER 65532:65532
ENTRYPOINT ["/away"]
