#!/bin/sh
# Makes the account the test signs in as, with the password it is told, and starts the
# server with a certificate made now.
set -eu
: "${ACCOUNT:?the name of the account to make}"
: "${PASSWORD_FILE:?a file that holds the account's password}"

useradd --create-home --shell /bin/sh "$ACCOUNT"
printf '%s:%s\n' "$ACCOUNT" "$(cat "$PASSWORD_FILE")" | chpasswd

openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=$(hostname)" \
  -keyout /etc/xrdp/key.pem -out /etc/xrdp/cert.pem 2>/dev/null
chown xrdp:xrdp /etc/xrdp/key.pem 2>/dev/null || true

# TLS, 32 bits a pixel, and the session that needs nothing but an X server
sed -i \
  -e 's|^security_layer=.*|security_layer=tls|' \
  -e 's|^certificate=.*|certificate=/etc/xrdp/cert.pem|' \
  -e 's|^key_file=.*|key_file=/etc/xrdp/key.pem|' \
  -e 's|^max_bpp=.*|max_bpp=32|' \
  /etc/xrdp/xrdp.ini

xrdp-sesman --nodaemon &
exec xrdp --nodaemon
