# Fixtures

Made for the tests and worth nothing else, by `make.sh` in this folder, for a domain
that doesn't exist: `CORP.EXAMPLE`.

| File | Is |
|---|---|
| `authority.der` | the authority that issued the others |
| `controller.der`, `controller.key.der` | a domain controller's certificate and its key: for the machines `localhost` and `dc-01.corp.example`, and good for answering a sign-in with a certificate |
| `account.der`, `account.key.der` | the certificate of the account `alex.urban@CORP.EXAMPLE` and its key, good for signing in |
| `stranger.der` | a certificate nobody issued but itself |
| `issuing.der`, `issuing.key.der` | a second authority and its key: what stands in for an organisation's authority in the tests issues certificates with it, and the test Kerberos server takes sign-ins with them |

The test Kerberos server of `tests/machines/kerberos/real.sh` takes the same files.

A test of what a ticket's proof is bound to holds a value worked out from
`controller.der`, apart from Away's code. After the certificates are made anew, the
test says the value it expects in their place.
