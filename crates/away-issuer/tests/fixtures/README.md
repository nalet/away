# Fixtures

Made for the tests and worth nothing else.

`sealed-by-a-browser.bin` is a password sealed as Away's page seals one (`web/src/kept.ts`),
with the WebCrypto of a browser's engine, to the key of
`../../away-server/tests/fixtures/keeping.pem`, under the name `mac-02`. The issuer's
tests open it with that key's private half, so that what a browser seals and what the
issuer opens stay one format.
