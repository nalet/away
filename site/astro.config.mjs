// Away's site: the product page at /, the documentation at /docs/, the specification at
// /spec/, all under the address in site.mjs.
//
//   npm run dev       copies the pages in (scripts/sync.mjs), dev server on :4321
//   npm run build     copies the pages in, static site -> dist/
//
// the pages are the docs/ and spec/ folders of this repository; the sections and their
// order are src/nav.mjs. the look is the nalet design system's: its tokens, its two
// typefaces, its dark canvas and square frames (src/styles/theme.css).
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { unified } from '@astrojs/markdown-remark';
import starlightSidebarTopics from 'starlight-sidebar-topics';
import starlightLinksValidator from 'starlight-links-validator';
import { remarkMermaid } from './src/lib/remark-mermaid.mjs';
import { topics, unlisted, gone } from './src/nav.mjs';
import { ORIGIN, BASE } from './site.mjs';
import codeTheme from './src/lib/code-theme.json' with { type: 'json' };

if (unlisted.length) {
  console.warn(`at the end of their section until src/nav.mjs lists them: ${unlisted.join(', ')}`);
}
if (gone.length) {
  console.warn(`listed in src/nav.mjs, gone from the repository: ${gone.join(', ')}`);
}

export default defineConfig({
  site: ORIGIN,
  base: BASE,
  trailingSlash: 'always',
  devToolbar: { enabled: false },
  markdown: {
    // straight quotes and dashes stay as they are written.
    processor: unified({ remarkPlugins: [remarkMermaid], smartypants: false }),
  },
  integrations: [
    starlight({
      title: 'Away',
      description: 'Away: remote access in a browser. What it is for, what it does, and how it is run.',
      favicon: '/favicon.svg',
      customCss: [
        '@fontsource/inter/400.css',
        '@fontsource/inter/500.css',
        '@fontsource/inter/600.css',
        '@fontsource/jetbrains-mono/400.css',
        '@fontsource/jetbrains-mono/500.css',
        '@fontsource/jetbrains-mono/600.css',
        '@fontsource/jetbrains-mono/700.css',
        './src/styles/tokens.css',
        './src/styles/theme.css',
      ],
      components: {
        Header: './src/components/Header.astro',
        SiteTitle: './src/components/SiteTitle.astro',
        Sidebar: './src/components/Sidebar.astro',
        ThemeProvider: './src/components/ThemeProvider.astro',
        ThemeSelect: './src/components/ThemeSelect.astro',
        MarkdownContent: './src/components/MarkdownContent.astro',
      },
      expressiveCode: {
        themes: [codeTheme],
        useStarlightUiThemeColors: false,
        styleOverrides: {
          borderRadius: '0',
          borderColor: 'var(--border)',
          codeFontFamily: 'var(--ff-mono)',
          codeFontSize: '13px',
          uiFontFamily: 'var(--ff-mono)',
          frames: {
            shadowColor: 'transparent',
            editorTabBarBackground: 'var(--bg-2)',
            terminalTitlebarBackground: 'var(--bg-2)',
            terminalBackground: 'var(--bg-2)',
            editorBackground: 'var(--bg-2)',
          },
        },
      },
      plugins: [
        // every page is in a section's side navigation (src/nav.mjs); the 404 page is in
        // none, the goal map (src/pages/spec/goals/) is in the goals', the capability map
        // (src/pages/spec/capabilities/) in the capabilities'.
        starlightSidebarTopics(topics, { exclude: ['/404'], topics: { goals: ['/spec/goals/**'], capabilities: ['/spec/capabilities/**'] } }),
        // the product page is outside the docs.
        starlightLinksValidator({ exclude: [`${BASE}/`] }),
      ],
    }),
  ],
});
