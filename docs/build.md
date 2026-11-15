# Building and checking Away

Away is built from its source with one command, and every image the project
publishes can be traced to the commit it was built from. This page says how to
do both.

What exists today: the program with its three parts, the pages, the chart
that installs it, and the pipelines. The parts say whether they are well, and
the server signs people in through a sign-in service and shows them the
machines a rule gives them. The gateway opens the desktop of a Windows machine
with Away's own RDP code, which a test program proves against real machines.
[The work packages](../spec/plan/README.md) say what comes next.

## Building it

It needs a Rust compiler, in the version that `rust-toolchain.toml` names,
Node 24 or newer with npm, and `make`. [rustup](https://rustup.rs/) installs
the right compiler by itself the first time it is asked.

```sh
make            # the pages, then the program with them inside, in target/release/away
make test       # every test of the program
make check      # what the pipeline checks on every change, short of a cluster
make image      # the image, as away:dev (needs Docker)
```

`make check` also needs Python 3.11 or newer and helm.

The pages are built first, in `web/`, and become part of the program: the
image holds one file, and the server reads nothing from a disk. A program
built without them, with `cargo build` alone, runs and says on its one page
that it holds none.

## Working on the pages

Two commands, in two terminals: the server beside a stand-in for a sign-in
service, which signs in whoever opens it without asking anything, and the
pages as they are written.

```sh
cargo run --package away-server --example pages --features stand-in
npm --prefix web run dev
```

The pages are then at the address the second command names, and change as
their files are saved. `ROLE=person` before the first command signs in
somebody who administers nothing.

## Testing the pages

A real browser goes through the pages as a person does: it is signed in, moves
between the views, uses the frame with the keyboard alone and on a phone's
screen, signs out, and meets a sign-in that failed and one that ran out. Every
page it sees has to pass the accessibility checks, WCAG 2.2 at the levels A
and AA, and may load nothing from anywhere but Away.

```sh
npx --prefix web playwright install chromium    # once
make test-pages
```

The tests run the server beside the stand-in for a sign-in service, and tell
the stand-in how to behave. The pipeline runs them on every change.

### Against a real sign-in service

`tests/machines/sign-in/real.sh` starts a real sign-in service in a container,
with a realm that knows Away as a client and holds one person, behind a
certificate of an authority made for that run. It starts Away's server beside
it, told to trust that authority, and a browser signs in by typing the
person's name and password on the service's own page. Then the person is
taken out of the administrators' group at the service, and Away has to know
with the next renewal; and the account is closed there, and Away has to ask
for a new sign-in.

```sh
make
tests/machines/sign-in/real.sh
```

It needs docker, openssl and curl. The pipeline runs it on every change.

The program is one file. Its first word chooses the part it runs as:

```sh
target/release/away server     # or: gateway, issuer
target/release/away help
```

A part writes what happens as lines on its output, one object a line, and
stops when it is sent the signal a cluster sends.

## Trying a machine

`away-try` reaches a machine with Away's own protocol code, with no browser
and nothing else of Away around it, and says what happened at each step. It
goes as far as that code goes. Today, over RDP, it asks a machine which
security it takes, secures the connection, reads the certificate the machine
shows, signs an account in before the desktop, takes the licence a machine for
many people asks it to take, opens the desktop, and takes what the machine
draws until the machine has drawn nothing for three seconds.
Then it says the size of the desktop's picture and the SHA-256 of its pixels
and, given a file, saves the picture there as a BMP.

```sh
cargo run --package away-try -- rdp fin-app-02.example.com
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password --save desktop.bmp
cargo run --package away-try -- rdp broker.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password --collection Desktops
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password --program '||ledger'
```

The password is read from a file, so that it is on no command line and in no
history.

With `--collection` the address is a farm's broker. `away-try` tells it which
collection is wanted, signs the account in, and says where the broker sends
the client: the machine's name, its address, the session it names, and what
the client is to say first there. It goes no further: it reaches one machine.

With `--program` it opens no desktop. It asks the machine to start that
program, says whether the program started, and names each window the program
opens: its title, where it is, and how large it is. A program that a farm
publishes is asked for by the name it is published under, with `||` before
it. What the machine draws is the windows, each at its place on a desktop
that is otherwise empty, and `--save` keeps that picture.

Windows signs the account in before the desktop unless it is told otherwise:
the password is proven and never sent in the clear, and the machine has to
prove that it holds the certificate it showed. A machine that signs the
account in itself, as RDP servers on Linux do, is told the name and the
password once the connection is secured. `away-try` takes whatever certificate
a machine shows and says its fingerprint, so use it with accounts that exist
for testing.

A Windows machine that serves desktops to many people asks every client to
take a licence before it shows anything. One that counts the people who use it
takes the client's answer and goes on; one that counts devices hands a licence
over. `away-try` says which happened. It keeps no licence, and it is the same
device to such a machine wherever it runs.

Leaving doesn't sign the account out: what runs on the desktop stays on the
machine for the account to come back to, until the account is signed out
there.

It is for the people who build Away, and it is not in the image.

### Checking the picture against the machine's screen

The picture is right when it is the machine's screen, pixel for pixel.
[`tests/machines/windows-screen.ps1`](../tests/machines/windows-screen.ps1)
runs on a Windows machine and writes, twice a second, the SHA-256 of what the
screen shows. It is computed the way `away-try` computes it: over the pixels,
three bytes each (blue, green, red), row by row from the top.

As an administrator of the machine, start it for the account:

```powershell
.\windows-screen.ps1 -ForAccount 'CORP\alex.urban' -Seconds 40
```

While it runs, take the picture:

```sh
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password
```

The script then writes its lines, one for each screen it took:

```
time,width,height,sha256
15:54:36.312,1280,800,4caeecc4fc2b4d1c030dcbf218ffc83602f00ebd0c2ca810c92ace46f55d4c3e
```

The picture is the machine's screen when the sum `away-try` says is the sum
of the machine's lines in the seconds before it left. With `-Pictures <folder>`
the script keeps each screen as a PNG file as well, to look at where a picture
differs.

A Windows Server offers its desktop at 16 bits a pixel unless it is set
otherwise, and a workstation at 32: `away-try` says which. The screen itself
has all its colours, so for a machine that offers 16, tell the script, and it
cuts each colour to the bits that travel before it makes the sum:

```powershell
.\windows-screen.ps1 -ForAccount 'CORP\alex.urban' -Seconds 40 -Bits 16
```

The script runs in the account's own session, so the account has to be signed
in on the machine when the script starts: take a picture once before. What
keeps the script from starting is a line as well. A machine that doesn't let
the account run PowerShell gives no lines at all. The script leaves nothing on
the machine; the account stays signed in until it is signed out there.

### Against an RDP server on Linux

`tests/machines/linux/rdp.sh` does the whole of it by itself: it starts an RDP
server on Linux in a container, with one account and a desktop that shows one
picture, signs in with `away-try`, and checks that the picture `away-try` took
is the server's screen, pixel for pixel. It needs docker.

```sh
cargo build --package away-try
tests/machines/linux/rdp.sh
```

The pipeline runs it on every change.

## Against a real database

What the server keeps is tried against a real PostgreSQL. A script starts one
in a container, with the tests' certificate for `localhost`, and runs the
tests that need it:

```sh
tests/machines/database/real.sh
```

`KEEP=1` before it leaves the database running at `localhost:15432`. The tests
find it by a variable, and so do the pages' tests, which then run a server
more, one that keeps what people star:

```sh
KEEP=1 tests/machines/database/real.sh
AWAY_TEST_DATABASE=localhost:15432 cargo test
AWAY_TEST_DATABASE=localhost:15432 make test-pages
```

Without the variable those tests say that they found no database, and pass.
The pipeline always has one, and fails where a test finds none.

With a database, a whole session is tried on every change: the server, the
gateway and a stand-in for a Windows machine run as they do in an installation,
and a browser opens the machine's desktop, types the account's password, sees
the machine's picture pixel for pixel, clicks and types, and ends the session.
The same world is there to work on the pages with:

```sh
KEEP=1 tests/machines/database/real.sh
DATABASE=localhost:15432 MACHINE_PORT=18096 cargo run --package away-server --example pages --features stand-in
```

The machine is `desk-01`, for the account `CORP\alex.urban` with the password
`a password that exists for these tests alone`.

## Against a real Kerberos server

Signing in on a Windows machine as oneself begins with Kerberos, and Away
speaks it with code of its own. That code is tried against a real Kerberos
server. A script starts one in a container, for the domain of the tests'
certificates, with signing in by certificate switched on, and runs the tests
that need it:

```sh
tests/machines/kerberos/real.sh
```

The tests ask it for a first ticket with the test account's certificate and
nothing else, and with that ticket for one for a machine's desktop. They also
ask as an account the server doesn't know, as another account than the
certificate's, and with a certificate nobody the server trusts has issued, and
each is refused; and they check that the client believes the server only when
the server's own certificate leads to the authority the client trusts.

`KEEP=1` before it leaves the server running at `localhost:1088`:

```sh
KEEP=1 tests/machines/kerberos/real.sh
AWAY_TEST_KERBEROS=localhost:1088 cargo test --package away-winauth
```

Without the variable those tests say that they found no Kerberos server, and
pass. The pipeline always has one, and fails where a test finds none. The
certificates are made by `crates/away-winauth/tests/fixtures/make.sh`, for a
domain that doesn't exist.

This is the first part of signing in as oneself. No page and no session uses
it yet.

## Trying a farm's feed

The server reads what a farm of Windows machines publishes from the farm's own
feed ([a farm](./machines.md#a-farm)). The same code, with nothing of Away
around it, says what a real farm publishes to an account:

```sh
cargo run --package away-server --example farm -- https://broker.corp.example/RDWeb/Feed/webfeed.aspx \
  --as 'CORP\svc-away' --password-file ~/.config/away/password
```

It says how many desktops and programs the farm publishes, and for each its
name, the address and the collection it is reached at, and the size of its
icon. `--authorities <file>` names the certificate of an authority to trust
besides the public ones, as PEM. The password is read from a file, and nothing
is printed of the account but its name.

The pages' tests run the server beside a stand-in for a farm's web access,
which signs its readers in as Windows does and publishes what a test tells it
to: `FARM_PORT` starts it with the pages' example.

## What a part answers

Every part answers three addresses on a listener of its own, port 9090, which
the way in never leads to:

| Address | Answers |
|---|---|
| `/livez` | 200 for as long as the part runs |
| `/readyz` | 200 once the part answers what it is there for; 503 before that and while it stops |
| `/metrics` | the part's measures, in the text format Prometheus reads |

The measures of this version:

| Measure | Says |
|---|---|
| `away_build_info` | the version that runs and the commit it was built from, as labels |
| `away_start_time_seconds` | when the part started |
| `away_ready` | whether the part answers what it is there for |
| `away_requests_total` | the requests a part answered, by listener and by the kind of answer |

## Testing an installation

`tests/install/kubernetes.sh` installs Away on the cluster that `kubectl`
points at, as someone with rights in one namespace alone, and checks that the
parts are well and that the cluster refuses every connection the architecture
doesn't list. It removes what it installed.

```sh
IMAGE_TAG=main tests/install/kubernetes.sh
```

With `INSTALLED=1` it checks an installation that is already there, with rights
in that namespace alone, and leaves it as it is. Given the address, it also
tries the way in from outside the cluster:

```sh
INSTALLED=1 NAMESPACE=away ADDRESS=away.example.com WAY_IN=router tests/install/kubernetes.sh
```

With `INGRESS`, the ingress controller's service as `namespace/name`, it
installs Away behind that controller, on an address and with a certificate
that it makes for the test from an authority of its own. It then tries the way
in through the controller, trusting that authority and no other, and checks
that no pod but the controller's reaches Away's address:

```sh
INGRESS=ingress/traefik INGRESS_CLASS=traefik tests/install/kubernetes.sh
```

It needs kubectl, helm, curl, jq and openssl, and a cluster that enforces
network rules. The pipeline runs it on every change, on a cluster it starts
for the purpose: once by itself, and once behind an ingress controller.

## Checking a published image

The pipeline publishes the image to `ghcr.io/nalet/away`: with the tag `main`
and one that names the commit from the main branch, and with a version's tag
for a release. Three things come with each image, and anyone can check them
without trusting the project's word.

**That the project's pipeline built it.** The image is signed with the
pipeline's own identity. Nobody holds a key.

```sh
cosign verify ghcr.io/nalet/away:main \
  --certificate-identity-regexp '^https://github\.com/nalet/away/\.github/workflows/build\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

**How it was built.** A record names the commit, the pipeline's file and the
run.

```sh
gh attestation verify oci://ghcr.io/nalet/away:main --repo nalet/away
```

**What it holds.** The image holds one file, the program, so its list of
contents is what the program was built from: every library with its version,
its licence and the checksum of its source.

```sh
gh attestation verify oci://ghcr.io/nalet/away:main --repo nalet/away \
  --predicate-type https://cyclonedx.org/bom --format json \
  | jq -r '.[0].verificationResult.statement.predicate.components[] | [.name, .version, .licenses[0].expression] | @tsv'
```

The chart is published beside the image, as `oci://ghcr.io/nalet/charts/away`,
and signed the same way:

```sh
cosign verify ghcr.io/nalet/charts/away:<version> \
  --certificate-identity-regexp '^https://github\.com/nalet/away/\.github/workflows/build\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

After every publication the pipeline runs these checks itself, with no right to
the registry, and installs the published chart on a fresh cluster.

## What the pipeline does

On every change, [the pipeline](../.github/workflows/build.yml):

1. checks that every library is listed with what it is for
   ([the libraries](./libraries.md));
2. checks the pages' code and builds them, to be part of the program, then
   checks the program's form and the compiler's warnings, which count as
   failures;
3. runs the tests, and goes through the pages in a browser;
4. renders the chart with each way in;
5. checks the libraries' licences, known security holes and sources;
6. builds the image, checks that it holds the program and nothing else, and
   installs it on a fresh Kubernetes cluster, where the test above tries it;
7. tries Away against real servers that it starts itself: a desktop over RDP
   on Linux, and a sign-in service.

On the main branch and on a version's tag it then publishes the image and the
chart, and checks what it published the way anyone can. Every tool the pipeline
uses is fixed to one commit.
