#!/bin/sh
# Starts the server with the password it is told, and puts the picture on its screen.
set -eu
: "${PASSWORD_FILE:?a file that holds the desktop's password}"

# the password as the server keeps it
vncpasswd -f < "$PASSWORD_FILE" > /run/vnc-password
chmod 600 /run/vnc-password

# the screen and the server in one: 1280 by 800, 24 bits of colour, the password and
# nothing else to sign in with
Xvnc :1 -geometry 1280x800 -depth 24 -rfbport 5900 \
  -SecurityTypes VncAuth -PasswordFile /run/vnc-password -AlwaysShared &
server=$!

# the picture on the screen, once the screen is there, and nothing that moves. The
# program that puts it there says it failed where it didn't, so the screen is asked
tries=0
until DISPLAY=:1 import -window root -depth 8 bgr:- >/dev/null 2>&1; do
  tries=$((tries + 1))
  [ "$tries" -lt 50 ] || { echo "The screen didn't start." >&2; exit 1; }
  sleep 0.2
done
DISPLAY=:1 display -window root /usr/local/share/desktop.png 2>/dev/null || true
shows=$(DISPLAY=:1 import -window root -depth 8 bgr:- | sha256sum)
[ "$shows" = "$(convert /usr/local/share/desktop.png -depth 8 bgr:- | sha256sum)" ] || { echo "The picture is not on the screen." >&2; exit 1; }
wait "$server"
