#!/bin/sh
# Tries Away's sign-in against a real sign-in service: starts one in a container, with a
# realm that knows Away as a client and holds one person, behind a certificate of an
# authority made for this run; starts Away's server beside it, told to trust that
# authority; and lets a browser sign in by typing the person's name and password.
#
#   make                                  # the program, with the pages inside
#   npx --prefix web playwright install chromium    # once
#   tests/machines/sign-in/real.sh
#
# It needs docker, openssl, curl and Node. AWAY names the program where it is not in
# target/release or target/debug; of those two it takes the one built last.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
away=${AWAY:-}
if [ -z "$away" ]; then
  # whichever was built last
  for built in "$root/target/release/away" "$root/target/debug/away"; do
    if [ -x "$built" ] && { [ -z "$away" ] || [ "$built" -nt "$away" ]; }; then away=$built; fi
  done
fi
[ -n "$away" ] || { echo "The program isn't built: run make." >&2; exit 1; }

image=quay.io/keycloak/keycloak:26.8.2@sha256:c68e286cacd87b942ac1939a0a1481461109d6acd3a57dc496969a319a76bbf0
name=away-test-sign-in
service=https://localhost:18443
at=http://127.0.0.1:18090
work=$(mktemp -d)
server=
ended() {
  [ -z "$server" ] || kill "$server" 2>/dev/null || true
  docker rm --force "$name" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap ended EXIT

# what only this run knows: the client's secret, the person's password, the administrator's
made() { od -An -N16 -tx1 /dev/urandom | tr -d ' \n'; }
secret=$(made)
password=$(made)
administrator=$(made)

echo "Making an authority for this run, and a certificate of it for the sign-in service"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 2 -subj "/CN=Away test authority" \
  -keyout "$work/authority.key" -out "$work/authority.pem" 2>/dev/null
openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -subj "/CN=localhost" \
  -keyout "$work/key.pem" -out "$work/request.pem" 2>/dev/null
printf 'subjectAltName=DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n' > "$work/for"
openssl x509 -req -in "$work/request.pem" -CA "$work/authority.pem" -CAkey "$work/authority.key" -CAcreateserial \
  -days 2 -extfile "$work/for" -out "$work/certificate.pem" 2>/dev/null
chmod 644 "$work/key.pem"

echo "Starting the sign-in service"
mkdir "$work/import"
sed -e "s|@SECRET@|$secret|" -e "s|@PASSWORD@|$password|" -e "s|@BACK@|$at/sign-in/back|" "$here/realm.json" > "$work/import/realm.json"
docker rm --force "$name" >/dev/null 2>&1 || true
docker create --name "$name" --publish 127.0.0.1:18443:8443 \
  --env KC_BOOTSTRAP_ADMIN_USERNAME=administrator --env KC_BOOTSTRAP_ADMIN_PASSWORD="$administrator" \
  "$image" start-dev --import-realm --hostname="$service" \
  --https-certificate-file=/opt/keycloak/conf/certificate.pem --https-certificate-key-file=/opt/keycloak/conf/key.pem >/dev/null
# the folder the service reads realms from is not in its image: it is brought whole
docker cp "$work/import" "$name:/opt/keycloak/data/import"
docker cp "$work/certificate.pem" "$name:/opt/keycloak/conf/certificate.pem"
docker cp "$work/key.pem" "$name:/opt/keycloak/conf/key.pem"
docker start "$name" >/dev/null

tries=0
until curl --silent --fail --cacert "$work/authority.pem" --output /dev/null "$service/realms/staff/.well-known/openid-configuration"; do
  tries=$((tries + 1))
  if [ "$tries" -ge 90 ]; then
    echo "The sign-in service didn't answer within three minutes." >&2
    docker logs "$name" 2>&1 | tail -n 40 >&2 || true
    exit 1
  fi
  sleep 2
done

echo "Starting Away's server beside it, told to trust that authority"
printf '%s\n' "$secret" > "$work/secret"
AWAY_ADDRESS=127.0.0.1:18090 AWAY_HEALTH_ADDRESS=127.0.0.1:18091 AWAY_PUBLIC_ADDRESS=$at \
  AWAY_SIGN_IN_SERVICE=$service/realms/staff AWAY_SIGN_IN_CLIENT=away AWAY_SIGN_IN_SECRET_FILE=$work/secret \
  AWAY_SIGN_IN_AUTHORITIES_FILE=$work/authority.pem AWAY_ADMINISTRATORS=away-admins AWAY_CONTACT=it-help@example.com \
  "$away" server > "$work/lines" 2>&1 &
server=$!
tries=0
until curl --silent --fail --output /dev/null "$at/api/who"; do
  tries=$((tries + 1))
  [ "$tries" -lt 30 ] || { echo "Away's server didn't answer." >&2; cat "$work/lines" >&2; exit 1; }
  sleep 1
done

echo "A browser signs in, by typing the person's name and password at the sign-in service"
if ! AWAY_AT=$at SERVICE_AT=$service ACCOUNT=alex.urban ACCOUNT_PASSWORD=$password ADMINISTRATOR_PASSWORD=$administrator \
  NODE_EXTRA_CA_CERTS=$work/authority.pem npx --prefix "$root/web" playwright test --config "$root/web/playwright.real.config.ts"; then
  echo "What Away's server wrote:" >&2
  cat "$work/lines" >&2
  exit 1
fi

echo "What Away's server wrote"
for happened in "signed in" "signed out" "a sign-in ran out"; do
  grep -q "\"event\":\"$happened\"" "$work/lines" || { echo "   It wrote no line for: $happened." >&2; cat "$work/lines" >&2; exit 1; }
  echo "   a line for: $happened"
done
if grep -q -e "$secret" -e "$password" -e "eyJ" "$work/lines"; then
  echo "   A secret is in what the server wrote." >&2
  exit 1
fi
echo "   and no secret, no password and no token in any line"
echo "Away signs people in through a real sign-in service."
