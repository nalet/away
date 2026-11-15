# Fixtures

Made for the tests and worth nothing else. `authority.pem` is the certificate of an
authority that exists for these tests; its key was thrown away once it had signed
`localhost.der`, a certificate for `localhost` whose key is `localhost.key.der`. The
tests' stand-in for a service presents the two.

`service.key.der` is a key that the tests' stand-in for a sign-in service signs with.
No sign-in service anywhere has it.
