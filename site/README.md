# The site

The site at <https://nalet.github.io/away/>: the product page at `/`, the
documentation at `/docs/`, the specification at `/spec/`.

The pages aren't written here. They are the `docs/` and `spec/` folders of this
repository, and every page has an "edit this page" link that leads there. This
folder holds the site around them: the product page, the two maps, the page of
all screens, the sections and their order, and the look.

| What | Where |
|---|---|
| The product page | [`src/pages/index.astro`](src/pages/index.astro) |
| The goal map | [`src/pages/spec/goals/map.astro`](src/pages/spec/goals/map.astro), [`src/lib/goalmap-lenses.mjs`](src/lib/goalmap-lenses.mjs) |
| The capability map and a fact sheet for each capability | [`src/pages/spec/capabilities/`](src/pages/spec/capabilities/), [`src/components/capabilities/`](src/components/capabilities/), [`src/lib/capmap-lenses.mjs`](src/lib/capmap-lenses.mjs) |
| The page of all screens, and a drawing shown small | [`src/pages/spec/screens/all.astro`](src/pages/spec/screens/all.astro), [`src/components/screens/Figure.astro`](src/components/screens/Figure.astro) |
| The model the maps are drawn from | [`src/lib/model.mjs`](src/lib/model.mjs), written by [`../scripts/model.py`](../scripts/model.py) `--json` |
| The sections and the order of their pages | [`src/nav.mjs`](src/nav.mjs) |
| How a page of the repository becomes a page of the site | [`scripts/sync.mjs`](scripts/sync.mjs) |
| The address the site is served at | [`site.mjs`](site.mjs) |
| The look: the nalet design system's tokens on [Starlight](https://starlight.astro.build) | [`src/styles/`](src/styles/), [`src/components/`](src/components/) |
| Publishing | [`../.github/workflows/site.yml`](../.github/workflows/site.yml) |

## Run it locally

```sh
npm ci
npm run dev          # copies the pages in, then http://localhost:4321/away/
npm run build        # the whole site in dist/, every internal link checked
```

It needs Node.js 22 or newer, and Python 3 for the model.

## How a page becomes a site page

- `spec/goals/README.md` is `/spec/goals/`; `docs/model.md` is `/docs/model/`.
  A folder's `README.md` is the folder's own page.
- The page's first `# heading` is its title.
- A link to another page goes to that page on the site. A link to any other
  file of the repository, such as a CSV file, goes to the file on GitHub.
- A ` ```mermaid ` block is drawn as a diagram.
- A picture is copied beside its page. Every picture is a drawing made at twice
  its size, so it is shown at half its pixels, and it gets an anchor after its
  file's name: `#fig-desktop-bar-computer`.

The build fails when a link leads nowhere, including a `#heading` that doesn't
exist, or when the model has a problem. The live site then stays as it was.
