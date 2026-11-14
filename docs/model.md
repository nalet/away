# Changing the model

The specification is a model: twelve tables, kept as CSV files, and pages whose
tables and numbers a program writes from them. To change what the specification
says, change a row; never a generated table. The screens are drawings, and
[changing a drawing](#changing-a-drawing) is below.

| File | One row is |
|---|---|
| [spec/goals/goals.csv](../spec/goals/goals.csv) | a goal: its kind, the goals it needs, why it is one |
| [spec/goals/qualities.csv](../spec/goals/qualities.csv) | a quality a goal has to be met with: what measures it, its target, a proposal for it |
| [spec/capabilities/areas.csv](../spec/capabilities/areas.csv) | an area of capabilities: its part, the goal it is for |
| [spec/capabilities/capabilities.csv](../spec/capabilities/capabilities.csv) | a capability: who it is for, and where it stands for RDP, VNC, SSH or Away itself |
| [spec/capabilities/expectations.csv](../spec/capabilities/expectations.csv) | something people expect of remote access, with the capabilities that meet it |
| [spec/screens/screens.csv](../spec/screens/screens.csv) | a figure of the screens: its chapter, what it shows, the capabilities it draws |
| [spec/architecture/components.csv](../spec/architecture/components.csv) | a component: its layer, its kind, when it comes, its code, what runs it, what it keeps |
| [spec/architecture/connections.csv](../spec/architecture/connections.csv) | a connection from one component to another: its kind, and what it is on |
| [spec/architecture/carries.csv](../spec/architecture/carries.csv) | a capability of the first release, with the components that carry it |
| [spec/architecture/decisions.csv](../spec/architecture/decisions.csv) | a decision record, with the components and the capabilities it governs |
| [spec/plan/packages.csv](../spec/plan/packages.csv) | a work package: a proof or a build, the packages it comes after, the decisions it rests on, its gate |
| [spec/plan/delivers.csv](../spec/plan/delivers.csv) | a capability that a package delivers, for every kind of connection or for the ones named |

What the columns and their values mean is in
[how to read the goals](../spec/goals/README.md#how-to-read-it),
[the capabilities](../spec/capabilities/README.md#how-to-read-it),
[the expectations](../spec/capabilities/expectations.md#how-to-read-it) and
[the architecture](../spec/architecture/README.md#how-to-read-it).

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
| add a capability to the first release, or move one into it | also name the components that carry it in `carries.csv` and the package that delivers it in `delivers.csv`: the check asks for both |
| accept a decision | change `Proposed` to `Accepted` in the record's status line, with the date |
| add a decision | write the record under `spec/architecture/decisions/` with the next number, and add its row to `decisions.csv` |
| move a capability to another work package | change its row in `delivers.csv` |
| agree to a work package, begin it, end it | change its `state` in `packages.csv` to `agreed`, `begun` or `done`; the check refuses one that has begun before the packages it comes after are done and its decisions accepted |

## Changing a drawing

Every screen is drawn in an HTML file beside its chapter, one file a chapter:
`spec/screens/img/ch06-desktop.html` holds the figures of
[chapter 6](../spec/screens/desktop.md). A figure's picture is made from it,
never edited itself.
[The drawings' sources](../spec/screens/README.md#17-the-drawings-sources) says
what the files are.

1. Edit the figure in its chapter's HTML file. Opened in a browser, the file
   shows every figure of the chapter.
2. Make the pictures again. The first line is needed once: the drawings take
   their fonts from the site's packages.

   ```sh
   (cd site && npm ci)
   npx -p playwright@1.63.0 node scripts/render-mockups.mjs ch06
   ```

3. Where what the figure shows has changed, change its text in the chapter and
   its row in `spec/screens/screens.csv`, then run `python3 scripts/model.py`.

| To | Do |
|---|---|
| add a figure | draw it as a new `<figure class="shot" id="…">`, make its picture, show it in the chapter with its caption, and add its row to `screens.csv` at its place in the chapter's order |
| say that a figure draws a capability | add the capability's key to the figure's row in `screens.csv` |
| remove a figure | remove it from the HTML file, the chapter and `screens.csv`, and delete its picture |

The check names a figure without a picture, a picture that is no figure, and a
caption whose number is not the figure's place in its chapter.

## The site

The site at <https://nalet.github.io/away/> is built from this repository on
every change to its main branch: the pages under `docs/` and `spec/`, the two
maps and the page of all screens, which are made from the same CSV files. To build it on one's own
machine:

```sh
cd site
npm ci
npm run dev      # http://localhost:4321/away/
npm run build    # the whole site in site/dist/, every link checked
```

It needs Node.js 22 or newer, and Python 3 for the maps.
