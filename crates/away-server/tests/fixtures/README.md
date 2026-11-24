# Fixtures

Made for the tests and worth nothing else. `files` holds files of machines and rules
as an installation gives them.
`keeping.pem` and `keeping.key.pem` are a certificate and its key, of RSA with 2048
bits, that the tests seal the secrets Away keeps to: the server reads the certificate,
the issuer the key. They secure nothing.
