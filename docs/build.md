# Building and checking Away

Away is built from its source with one command, and every image the project
publishes can be traced to the commit it was built from. This page says how to
do both.

What exists today: the program with its three parts, the pages, the chart
that installs it, and the pipelines. The parts say whether they are well, and
the server signs people in through a sign-in service and shows them the frame
of Away's pages, which have no machine to open yet. Away's own RDP code is
proven by a test program. [The work packages](../spec/plan/README.md) say what
comes next.

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
shows, signs an account in before the desktop, opens the desktop, and takes
what the machine draws until the machine has drawn nothing for three seconds.
Then it says the size of the desktop's picture and the SHA-256 of its pixels
and, given a file, saves the picture there as a BMP.

```sh
cargo run --package away-try -- rdp fin-app-02.example.com
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password
cargo run --package away-try -- rdp fin-app-02.example.com --as 'CORP\alex.urban' --password-file ~/.config/away/password --save desktop.bmp
```

The password is read from a file, so that it is on no command line and in no
history.

Windows signs the account in before the desktop unless it is told otherwise:
the password is proven and never sent in the clear, and the machine has to
prove that it holds the certificate it showed. A machine that signs the
account in itself, as RDP servers on Linux do, is told the name and the
password once the connection is secured. `away-try` takes whatever certificate
a machine shows and says its fingerprint, so use it with accounts that exist
for testing.

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

The script runs in the account's own session, so the account has to have
signed in on the machine once before. It leaves nothing on the machine.

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

It needs kubectl, helm, curl and jq, and a cluster that enforces network
rules. The pipeline runs it on every change, on a cluster it starts for the
purpose.

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
3. runs the tests;
4. renders the chart with each way in;
5. checks the libraries' licences, known security holes and sources;
6. builds the image, checks that it holds the program and nothing else, and
   installs it on a fresh Kubernetes cluster, where the test above tries it;
7. tries Away's own protocol code against a real server that it starts itself:
   a desktop over RDP on Linux.

On the main branch and on a version's tag it then publishes the image and the
chart, and checks what it published the way anyone can. Every tool the pipeline
uses is fixed to one commit.
