# Away

Remote access in a browser: a Windows desktop or one of its programs, a Mac or
a Linux desktop, a shell. Signed in once, with the organisation's own sign-in,
and then signed in on the machine too.

**Specified, and being built. Nothing can be installed yet.** What Away is for,
what it has to do, what it looks like, how it is built and in what order are
written down, drawn and agreed. It is built one work package at a time. The
first is done: the program with its three parts, the chart that installs it and
the pipelines. The second has begun: Away's own RDP code, proven against real
machines with no browser.
<https://nalet.github.io/away/>

| | In this repository | On the site |
|---|---|---|
| What it is for | [spec/goals](spec/goals/README.md) | [the goal map](https://nalet.github.io/away/spec/goals/map/) |
| What it does, and when | [spec/capabilities](spec/capabilities/README.md) | [the capability map](https://nalet.github.io/away/spec/capabilities/map/) |
| What people expect of it | [expectations](spec/capabilities/expectations.md) | |
| What it leaves out | [what is left out](spec/capabilities/left-out.md) | |
| What it looks like | [spec/screens](spec/screens/README.md) | [all screens](https://nalet.github.io/away/spec/screens/all/) |
| How it is built, and why | [spec/architecture](spec/architecture/README.md) | [the architecture](https://nalet.github.io/away/spec/architecture/) |
| In what order it is built | [spec/plan](spec/plan/README.md) | [the work packages](https://nalet.github.io/away/spec/plan/) |

## Building it

```sh
make          # the program, in target/release/away
make test     # every test of the program
make check    # what the pipeline checks on every change, short of a cluster
```

It needs a Rust compiler, which [rustup](https://rustup.rs/) installs in the
version this repository names. [Building and checking](docs/build.md) says how
to check an image the project published, and [installing](docs/install.md) how
to install one. A security hole is reported [in private](SECURITY.md).

## The model

The specification is a model: tables kept as CSV files under `spec/`, and pages
whose tables and numbers are written from them.

```sh
python3 scripts/model.py           # check the tables, write the pages
python3 scripts/model.py --check   # check only; fails when a page is out of date
```

[Changing the model](docs/model.md) has the steps. The site is in `site/`, and
is published from the main branch.

## Licence

[Mozilla Public License 2.0](LICENSE).
