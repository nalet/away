#!/bin/sh
# Tries Away's own RDP code against an RDP server on Linux: starts the server in a
# container, signs in with a typed name and password, takes the desktop's picture, and
# checks that it is the server's screen, pixel for pixel.
#
#   cargo build --package away-try
#   tests/machines/linux/rdp.sh
#
# It needs docker. TRY names the test program where it is not in target/debug, and PORT
# the port of this machine the server is reached on.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
try=${TRY:-$here/../../../target/debug/away-try}
port=${PORT:-13389}
name=away-test-rdp-linux
work=$(mktemp -d)
trap 'docker rm --force "$name" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

# an account that lives as long as the container, with a password made now
od -An -N16 -tx1 /dev/urandom | tr -d ' \n' > "$work/password"

echo "Starting an RDP server on Linux"
docker build --quiet --tag "$name" "$here" >/dev/null
docker rm --force "$name" >/dev/null 2>&1 || true
docker create --name "$name" --publish "127.0.0.1:$port:3389" \
  --env ACCOUNT=alex --env PASSWORD_FILE=/run/away-password "$name" >/dev/null
docker cp "$work/password" "$name:/run/away-password"
docker start "$name" >/dev/null

tries=0
until "$try" rdp "127.0.0.1:$port" >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -ge 60 ]; then
    echo "The server didn't answer within a minute." >&2
    docker logs "$name" >&2 || true
    exit 1
  fi
  sleep 1
done

# the first sign-in starts the desktop, and may be there before its picture is: a
# picture of one colour is taken once more
black() {
  head -c "$(( $1 * $2 * 3 ))" /dev/zero | sha256sum | cut -d' ' -f1
}
for attempt in 1 2 3; do
  "$try" rdp "127.0.0.1:$port" --as alex --password-file "$work/password" --save "$work/desktop.bmp" | tee "$work/said"
  took=$(sed -n 's/.*Their SHA-256: //p' "$work/said")
  size=$(sed -n 's/^ *\([0-9]*\) by \([0-9]*\) pixels\..*/\1 \2/p' "$work/said")
  if [ -n "$took" ] && [ "$took" != "$(black ${size% *} ${size#* })" ]; then
    break
  fi
  echo "The desktop was still empty. Once more."
done

echo "What the server's own screen shows"
shows=$(docker exec --user alex "$name" sh -c 'DISPLAY=:10 import -window root -depth 8 bgr:- | sha256sum' | cut -d' ' -f1)
echo "   The SHA-256 of its pixels: $shows"

if [ -z "$took" ] || [ "$took" = "$(black ${size% *} ${size#* })" ]; then
  echo "The picture is empty." >&2
  exit 1
fi
if [ "$took" != "$shows" ]; then
  echo "The picture is not the server's screen." >&2
  exit 1
fi
echo "The picture is the server's screen, pixel for pixel."
