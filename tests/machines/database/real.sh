#!/bin/sh
# Tries the server's own way of speaking to a database against a real PostgreSQL: starts
# one in a container, with a database and an account for the tests and the tests'
# certificate for localhost, and runs the tests that need it.
#
#   tests/machines/database/real.sh
#
# It needs docker, openssl and cargo. KEEP=1 leaves the database running afterwards, at
# localhost:15432, for running the tests by hand:
#
#   AWAY_TEST_DATABASE=localhost:15432 cargo test --package away-server --test database
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
image=postgres:17.12-alpine@sha256:d3244e8ce3f165ee3c93d9c4a4b1d0fe48b3a89e2287b717fdcbbde92d405065
name=away-test-database
port=15432
work=$(mktemp -d)
ended() {
  [ -n "${KEEP:-}" ] || docker rm --force "$name" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap ended EXIT

echo "The tests' certificate for localhost, as a database takes it"
fixtures=$root/crates/away-part/tests/fixtures
openssl x509 -inform der -in "$fixtures/localhost.der" -out "$work/server.crt"
openssl pkey -inform der -in "$fixtures/localhost.key.der" -out "$work/server.key"

echo "Starting PostgreSQL"
docker rm --force "$name" >/dev/null 2>&1 || true
# the database takes a key only from a file of its own account, so the two are put into
# the container before it starts, and copied there to where the database reads them
docker create --name "$name" --publish "127.0.0.1:$port:5432" \
  --env POSTGRES_USER=away --env POSTGRES_DB=away --env "POSTGRES_PASSWORD=a password for this machine alone" \
  --entrypoint sh "$image" -c '
    cp /given/server.crt /given/server.key /var/lib/postgresql/ &&
    chown postgres /var/lib/postgresql/server.crt /var/lib/postgresql/server.key &&
    chmod 600 /var/lib/postgresql/server.key &&
    exec docker-entrypoint.sh postgres -c ssl=on \
      -c ssl_cert_file=/var/lib/postgresql/server.crt -c ssl_key_file=/var/lib/postgresql/server.key' >/dev/null
docker cp "$work/." "$name:/given"
docker start "$name" >/dev/null

# the container starts the database twice: once to make it, then for good
ready=
for _ in $(seq 1 120); do
  if docker exec "$name" psql --username away --dbname away --host 127.0.0.1 --tuples-only --command 'select 1' >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.5
done
[ -n "$ready" ] || { echo "The database did not start:" >&2; docker logs "$name" 2>&1 | tail -20 >&2; exit 1; }
echo "   $(docker exec "$name" psql --username away --dbname away --tuples-only --no-align --command 'show server_version')"

echo "The tests"
cd "$root"
out=$work/tests.txt
AWAY_TEST_DATABASE=localhost:$port cargo test --locked --package away-server --test database -- --nocapture 2>&1 | tee "$out"
grep -q '^test result: ok' "$out" || { echo "The tests failed." >&2; exit 1; }
# a test that found no database passes without testing anything: here that is a failure
if grep -q 'no database to test against' "$out"; then
  echo "The tests did not see the database." >&2
  exit 1
fi
