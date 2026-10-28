import { defineConfig } from 'vitepress'

const site = 'https://paulvandermeijs.github.io/treehouse/'

export default defineConfig({
  title: 'Treehouse',
  description: 'Run Pi coding agents in git worktrees.',
  // GitHub Pages serves the site from /treehouse/.
  base: '/treehouse/',
  cleanUrls: true,
  lastUpdated: true,
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/treehouse/logo-latte.svg', media: '(prefers-color-scheme: light)' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/treehouse/logo-mocha.svg', media: '(prefers-color-scheme: dark)' }],
    // A link preview can't follow the reader's theme, so it gets the Mocha poster.
    ['meta', { property: 'og:image', content: `${site}treehouse-poster-mocha.png` }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
  ],
  themeConfig: {
    logo: { light: '/logo-latte.svg', dark: '/logo-mocha.svg' },
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Development', link: '/development/' },
    ],
    sidebar: [
      {
        text: 'Getting started',
        items: [
          { text: 'Install and run', link: '/guide/getting-started' },
          { text: 'Projects and worktrees', link: '/guide/worktrees' },
        ],
      },
      {
        text: 'Working with Pi',
        items: [
          { text: 'The chat', link: '/guide/chat' },
          { text: 'Slash and shell commands', link: '/guide/commands' },
          { text: 'Sub-agents', link: '/guide/sub-agents' },
          { text: 'Reviewing the diff', link: '/guide/diff' },
          { text: 'Committing and merging', link: '/guide/lifecycle' },
        ],
      },
      {
        text: 'Safety',
        items: [
          { text: 'Approvals', link: '/guide/approvals' },
          { text: 'Project trust', link: '/guide/trust' },
          { text: 'MCP servers', link: '/guide/mcp' },
        ],
      },
      {
        text: 'Reference',
        items: [
          { text: 'Settings', link: '/guide/settings' },
          { text: 'Keyboard shortcuts', link: '/guide/shortcuts' },
          { text: 'Packaging', link: '/guide/packaging' },
        ],
      },
      {
        text: 'Development',
        items: [
          { text: 'Architecture', link: '/development/' },
          { text: 'Running without a model', link: '/development/fake-pi' },
        ],
      },
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/paulvandermeijs/treehouse' },
      { icon: 'discord', link: 'https://discord.gg/hzE6muNPh6' },
    ],
    editLink: {
      pattern: 'https://github.com/paulvandermeijs/treehouse/edit/master/docs/:path',
    },
    search: { provider: 'local' },
    footer: {
      message: 'Released under the MIT License.',
    },
    outline: 'deep',
  },
})
