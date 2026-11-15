#!/bin/sh
# Makes the certificates the tests of the Windows sign-in use, with OpenSSL: an authority,
# a domain controller's certificate, an account's with its key, and one that belongs to
# nothing. They are for a domain that doesn't exist, CORP.EXAMPLE, and are good for a
# hundred years, so that no test fails for the date.
#
#   crates/away-winauth/tests/fixtures/make.sh
#
# The same files serve the test Kerberos server of tests/machines/kerberos/real.sh.
set -eu
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cat > "$work/extensions" <<'END'
[authority]
basicConstraints = critical, CA:TRUE
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash

[controller]
basicConstraints = CA:FALSE
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = 1.3.6.1.5.2.3.5, serverAuth
subjectAltName = DNS:localhost, DNS:dc-01.corp.example, otherName:1.3.6.1.5.2.2;SEQUENCE:controllers_name

[controllers_name]
realm = EXP:0, GeneralString:CORP.EXAMPLE
name = EXP:1, SEQUENCE:controllers_names

[controllers_names]
kind = EXP:0, INTEGER:1
parts = EXP:1, SEQUENCE:controllers_parts

[controllers_parts]
first = GeneralString:krbtgt
second = GeneralString:CORP.EXAMPLE

[account]
basicConstraints = CA:FALSE
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = 1.3.6.1.4.1.311.20.2.2, clientAuth
subjectAltName = otherName:1.3.6.1.4.1.311.20.2.3;UTF8:alex.urban@CORP.EXAMPLE
END

key() { openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$work/$1.key" 2>/dev/null; }
key authority
openssl req -x509 -new -key "$work/authority.key" -sha256 -days 36500 -subj "/DC=example/DC=corp/CN=corp-authority" \
  -config "$work/extensions" -extensions authority -out "$work/authority.pem"
for one in controller account; do
  key "$one"
  case "$one" in controller) subject="/CN=dc-01.corp.example" ;; *) subject="/DC=example/DC=corp/CN=Users/CN=Alex Urban" ;; esac
  openssl req -new -key "$work/$one.key" -subj "$subject" -out "$work/$one.csr"
  openssl x509 -req -in "$work/$one.csr" -CA "$work/authority.pem" -CAkey "$work/authority.key" -CAcreateserial -sha256 -days 36500 \
    -extfile "$work/extensions" -extensions "$one" -out "$work/$one.pem" 2>/dev/null
done
key stranger
openssl req -x509 -new -key "$work/stranger.key" -sha256 -days 36500 -subj "/CN=somebody else" -out "$work/stranger.pem"

for one in authority controller account stranger; do
  openssl x509 -in "$work/$one.pem" -outform der -out "$one.der"
done
for one in controller account; do
  openssl pkcs8 -topk8 -nocrypt -in "$work/$one.key" -outform der -out "$one.key.der"
done
ls -l ./*.der
