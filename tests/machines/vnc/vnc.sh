#!/bin/sh
# Tries Away's own VNC code against a VNC server on Linux: starts the server in a
# container, signs in with its password, takes the desktop's picture, and checks that
# it is the server's screen, pixel for pixel. A wrong password has to be refused, and the
# server has to answer the extension for a clipboard of more than western text.
#
#   cargo build --package away-try
#   tests/machines/vnc/vnc.sh
#
# It needs docker. TRY names the test program where it is not in target/debug, and PORT
# the port of this machine the server is reached on.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
try=${TRY:-$here/../../../target/debug/away-try}
port=${PORT:-15900}
name=away-test-vnc-linux
work=$(mktemp -d)
trap 'docker rm --force "$name" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

# a password that lives as long as the container, made now: eight characters, which is
# all a server of this kind looks at
od -An -N4 -tx1 /dev/urandom | tr -d ' \n' > "$work/password"
printf 'another1' > "$work/wrong"

echo "Starting a VNC server on Linux"
docker build --quiet --tag "$name" "$here" >/dev/null
docker rm --force "$name" >/dev/null 2>&1 || true
docker create --name "$name" --publish "127.0.0.1:$port:5900" \
  --env PASSWORD_FILE=/run/away-password "$name" >/dev/null
docker cp "$work/password" "$name:/run/away-password"
docker start "$name" >/dev/null

tries=0
until "$try" vnc "127.0.0.1:$port" >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -ge 60 ]; then
    echo "The server didn't answer within a minute." >&2
    docker logs "$name" >&2 || true
    exit 1
  fi
  sleep 1
done

# the server may answer before its picture is on the screen: a picture of one colour is
# taken once more
black() {
  head -c "$(( $1 * $2 * 3 ))" /dev/zero | sha256sum | cut -d' ' -f1
}
for attempt in 1 2 3; do
  "$try" vnc "127.0.0.1:$port" --password-file "$work/password" --save "$work/desktop.bmp" | tee "$work/said"
  took=$(sed -n 's/.*Their SHA-256: //p' "$work/said")
  size=$(sed -n 's/^ *\([0-9]*\) by \([0-9]*\) pixels\. Their.*/\1 \2/p' "$work/said")
  if [ -n "$took" ] && [ "$took" != "$(black ${size% *} ${size#* })" ]; then
    break
  fi
  echo "The desktop was still empty. Once more."
done

echo "What the server's own screen shows"
shows=$(docker exec "$name" sh -c 'DISPLAY=:1 import -window root -depth 8 bgr:- | sha256sum' | cut -d' ' -f1)
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

echo "A wrong password"
if "$try" vnc "127.0.0.1:$port" --password-file "$work/wrong" > "$work/refused" 2>&1; then
  echo "The server took a wrong password." >&2
  exit 1
fi
grep -q "did not take the password" "$work/refused" || { cat "$work/refused" >&2; echo "It was refused for another reason." >&2; exit 1; }
echo "   It was refused."

# a server of this kind carries a clipboard with more than western text, and says so
# when it is named the extension
echo "What it knows beyond the protocol's core"
"$try" vnc "127.0.0.1:$port" --password-file "$work/password" --extensions | tee "$work/knows"
if ! awk '/It answered these/ { on = 1 } /It said nothing to these/ { on = 0 } on' "$work/knows" | grep -q "ExtendedClipboard"; then
  echo "The server did not answer the clipboard's extension." >&2
  exit 1
fi
echo "It answered the clipboard's extension."
