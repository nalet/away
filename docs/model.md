# Changing the model

The specification is a model: five tables, kept as CSV files, and pages whose
tables and numbers a program writes from them. To change what the specification
says, change a row; never a generated table.

| File | One row is |
|---|---|
| [spec/goals/goals.csv](../spec/goals/goals.csv) | a goal: its kind, the goals it needs, why it is one |
| [spec/goals/qualities.csv](../spec/goals/qualities.csv) | a quality a goal has to be met with: what measures it, its target, a proposal for it |
| [spec/capabilities/areas.csv](../spec/capabilities/areas.csv) | an area of capabilities: its part, the goal it is for |
| [spec/capabilities/capabilities.csv](../spec/capabilities/capabilities.csv) | a capability: who it is for, and where it stands for RDP, VNC, SSH or Away itself |
| [spec/capabilities/expectations.csv](../spec/capabilities/expectations.csv) | something people expect of remote access, with the capabilities that meet it |

What the columns and their values mean is in
[how to read the goals](../spec/goals/README.md#how-to-read-it),
[the capabilities](../spec/capabilities/README.md#how-to-read-it) and
[the expectations](../spec/capabilities/expectations.md#how-to-read-it).

## The steps

1. Edit the CSV file. A spreadsheet program works, and so does a text editor; a
   cell that holds a comma is in double quotes.
2. Run the program. It checks the tables and writes the pages:

   ```sh
   python3 scripts/model.py
   ```

   It names every problem it finds, such as an expectation that names a
   capability that doesn't exist, and ends with the counts. It needs Python 3
   and nothing else.
3. Look at what changed in the pages under `spec/`, and commit the CSV files
   and the pages together.

On every change, `python3 scripts/model.py --check` runs by itself. It fails
when the model has a problem, or when a page's tables are not what the CSV
files say.

## Common changes

| To | Do |
|---|---|
| move a capability into the first release, or out of it | change its cell for RDP, VNC, SSH or Away in `capabilities.csv` to `v1` or `later` |
| leave a capability out | set the cell to `no`, and say why in its note |
| set a quality's target | fill `target` in `qualities.csv`; `proposal` stays as the suggestion it was |
| strike a goal | remove its row from `goals.csv`, then the areas, expectations and qualities that are for it: the check names each |
| add a capability | add a row with a new key, in an area that exists; name it in an expectation if one is met by it |

## The site

The site at <https://nalet.github.io/away/> is built from this repository on
every change to its main branch: the pages under `docs/` and `spec/`, and the
two maps, which are drawn from the same CSV files. To build it on one's own
machine:

```sh
cd site
npm ci
npm run dev      # http://localhost:4321/away/
npm run build    # the whole site in site/dist/, every link checked
```

It needs Node.js 22 or newer, and Python 3 for the maps.
