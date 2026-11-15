# Fixtures

Made for the tests and worth nothing else, by `make.sh` in this folder, for a domain
that doesn't exist: `CORP.EXAMPLE`.

| File | Is |
|---|---|
| `authority.der` | the authority that issued the others |
| `controller.der`, `controller.key.der` | a domain controller's certificate and its key: for the machines `localhost` and `dc-01.corp.example`, and good for answering a sign-in with a certificate |
| `account.der`, `account.key.der` | the certificate of the account `alex.urban@CORP.EXAMPLE` and its key, good for signing in |
| `stranger.der` | a certificate nobody issued but itself |

The test Kerberos server of `tests/machines/kerberos/real.sh` takes the same files.
