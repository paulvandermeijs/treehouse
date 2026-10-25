---
layout: home

hero:
  name: Treehouse
  text: Run Pi coding agents in git worktrees
  tagline: Each worktree gets its own agent. You approve what runs, follow its sub-agents and review the diff.
  image:
    light: /logo-latte.svg
    dark: /logo-mocha.svg
    alt: Treehouse
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: View on GitHub
      link: https://github.com/paulvandermeijs/treehouse

features:
  - title: One worktree, one agent
    details: Every chat runs Pi in its own git worktree, so agents never step on each other's changes.
    link: /guide/worktrees
  - title: Nothing runs without you
    details: Commands wait until you run, always allow or deny them. Deleting and pushing always ask.
    link: /guide/approvals
  - title: Sub-agents you can steer
    details: Pi hands side quests to sub-agents. Open one to follow along, steer it or stop it.
    link: /guide/sub-agents
  - title: Review the diff
    details: See each worktree's changes next to the chat, unified or side by side.
    link: /guide/diff
---

<div class="announcement">
  <img loading="lazy" class="light-only" src="/treehouse-announcement-latte.gif" alt="Treehouse running Pi agents in worktrees">
  <img loading="lazy" class="dark-only" src="/treehouse-announcement-mocha.gif" alt="Treehouse running Pi agents in worktrees">
</div>
