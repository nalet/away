#!/bin/sh
# Tries Away's own Kerberos against a real Kerberos server: starts one in a container,
# for the domain of the tests' certificates, CORP.EXAMPLE, with certificate sign-in
# switched on, two accounts that sign in with a certificate, one that signs in with a
# password, a machine's service and the service of an authority's pages, and runs the
# tests that need it. Beside it, in
# the same container, a machine takes the tickets for that service, with the Kerberos
# library's own code (machine.py).
#
#   tests/machines/kerberos/real.sh
#
# It needs docker, openssl and cargo. KEEP=1 leaves both running afterwards, the server
# at localhost:1088 and the machine at localhost:1089, for running the tests by hand:
#
#   AWAY_TEST_KERBEROS=localhost:1088 AWAY_TEST_KERBEROS_MACHINE=localhost:1089 \
#     cargo test --package away-winauth --test kerberos
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
image=alpine:3.23@sha256:469c9ea410b9215eaee7976ba6c0639039b444be8e5d638f7abbada189971eb2
name=away-test-kerberos
port=1088
machine_port=1089
work=$(mktemp -d)
ended() {
  [ -n "${KEEP:-}" ] || docker rm --force "$name" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap ended EXIT

echo "The tests' authority and the domain controller's certificate, as the server takes them"
fixtures=$root/crates/away-winauth/tests/fixtures
openssl x509 -inform der -in "$fixtures/authority.der" -out "$work/authority.pem"
openssl x509 -inform der -in "$fixtures/controller.der" -out "$work/controller.pem"
# the authority that issues certificates in the tests, a moment before they are used
openssl x509 -inform der -in "$fixtures/issuing.der" -out "$work/issuing.pem"
openssl pkey -inform der -in "$fixtures/controller.key.der" -out "$work/controller.key"

cat > "$work/krb5.conf" <<'END'
[libdefaults]
  default_realm = CORP.EXAMPLE
  dns_lookup_kdc = false
  dns_lookup_realm = false
[realms]
  CORP.EXAMPLE = {
    kdc = localhost
  }
END
# a certificate names its account the way a Windows domain's does, and is good for
# signing in with a smart card: the server is told to take both
cat > "$work/kdc.conf" <<'END'
[kdcdefaults]
  kdc_listen = 88
  kdc_tcp_listen = 88
[realms]
  CORP.EXAMPLE = {
    database_name = /var/lib/krb5kdc/principal
    key_stash_file = /var/lib/krb5kdc/stash
    supported_enctypes = aes256-cts-hmac-sha1-96:normal aes128-cts-hmac-sha1-96:normal
    max_life = 10h
    pkinit_identity = FILE:/given/controller.pem,/given/controller.key
    pkinit_anchors = FILE:/given/authority.pem
    pkinit_anchors = FILE:/given/issuing.pem
    pkinit_allow_upn = true
    pkinit_eku_checking = scLogin
  }
[logging]
  kdc = STDERR
END
cat > "$work/start.sh" <<'END'
set -eu
apk add --no-cache --quiet krb5-server krb5 krb5-pkinit py3-python-gssapi >/dev/null
cp /given/krb5.conf /etc/krb5.conf
mkdir -p /var/lib/krb5kdc
cp /given/kdc.conf /var/lib/krb5kdc/kdc.conf
export KRB5_KDC_PROFILE=/var/lib/krb5kdc/kdc.conf
kdb5_util create -s -r CORP.EXAMPLE -P "a password for this machine alone" >/dev/null 2>&1
# accounts that are signed in with a certificate alone: their keys are random
for account in alex.urban sam.other; do
  kadmin.local -q "addprinc -randkey +requires_preauth $account@CORP.EXAMPLE" >/dev/null 2>&1
done
# and one that is signed in with a password
kadmin.local -q "addprinc -pw \"a password that exists for these tests alone\" +requires_preauth pat.typed@CORP.EXAMPLE" >/dev/null 2>&1
kadmin.local -q "addprinc -randkey TERMSRV/desk-01.corp.example@CORP.EXAMPLE" >/dev/null 2>&1
# a machine the tests reach by its address, and stand in for themselves: its service's
# key is made from a password, so that they can hold it
kadmin.local -q "addprinc -pw \"the password of a machine that exists for these tests alone\" TERMSRV/127.0.0.1@CORP.EXAMPLE" >/dev/null 2>&1
kadmin.local -q "addprinc -pw \"the password of a machine that exists for these tests alone\" TERMSRV/localhost@CORP.EXAMPLE" >/dev/null 2>&1
# an authority's pages that the tests stand in for themselves, reached as localhost: the
# key of their service is made from a password too
kadmin.local -q "addprinc -pw \"the password of pages that exist for these tests alone\" HTTP/localhost@CORP.EXAMPLE" >/dev/null 2>&1
# the machine that takes the tickets for that service holds the service's key
kadmin.local -q "ktadd -k /etc/krb5.keytab TERMSRV/desk-01.corp.example@CORP.EXAMPLE" >/dev/null 2>&1
python3 /given/machine.py &
echo "the server is ready"
exec krb5kdc -n
END

echo "Starting the Kerberos server"
docker rm --force "$name" >/dev/null 2>&1 || true
docker create --name "$name" --publish "127.0.0.1:$port:88" --publish "127.0.0.1:$machine_port:4088" --entrypoint sh "$image" /given/start.sh >/dev/null
cp "$here/machine.py" "$work/machine.py"
docker cp "$work/." "$name:/given"
docker start "$name" >/dev/null

ready=
for _ in $(seq 1 120); do
  if docker logs "$name" > "$work/log.txt" 2>&1 && grep -q "the server is ready" "$work/log.txt" && grep -q "the machine is ready" "$work/log.txt"; then
    ready=1
    break
  fi
  sleep 0.5
done
[ -n "$ready" ] || { echo "The Kerberos server did not start:" >&2; docker logs "$name" 2>&1 | tail -20 >&2; exit 1; }
echo "   $(docker exec "$name" sh -c 'apk info -e -v krb5-server 2>/dev/null' | head -1)"

echo "The tests"
cd "$root"
out=$work/tests.txt
AWAY_TEST_KERBEROS=localhost:$port AWAY_TEST_KERBEROS_MACHINE=localhost:$machine_port \
  cargo test --locked --package away-winauth --test kerberos -- --nocapture 2>&1 | tee "$out"
grep -q '^test result: ok' "$out" || { echo "The tests failed." >&2; docker logs "$name" 2>&1 | tail -30 >&2; exit 1; }
# a test that found no server passes without testing anything: here that is a failure
if grep -q -e 'no Kerberos server to test against' -e 'no machine to show a ticket to' "$out"; then
  echo "The tests did not see the server." >&2
  exit 1
fi
