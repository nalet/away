# Away

Remote access in a browser: a Windows desktop or one of its programs, a Mac or
a Linux desktop, a shell. Signed in once, with the organisation's own sign-in,
and then signed in on the machine too.

**Being specified. Nothing is built yet.** What Away is for, what it has to do
and what it looks like are written down and drawn as a first cut, and under
review:
<https://nalet.github.io/away/>

| | In this repository | On the site |
|---|---|---|
| What it is for | [spec/goals](spec/goals/README.md) | [the goal map](https://nalet.github.io/away/spec/goals/map/) |
| What it does, and when | [spec/capabilities](spec/capabilities/README.md) | [the capability map](https://nalet.github.io/away/spec/capabilities/map/) |
| What people expect of it | [expectations](spec/capabilities/expectations.md) | |
| What it leaves out | [what is left out](spec/capabilities/left-out.md) | |
| What it looks like | [spec/screens](spec/screens/README.md) | [all screens](https://nalet.github.io/away/spec/screens/all/) |

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
