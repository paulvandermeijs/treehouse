<template>
  <div class="early-banner">
    <p>
      Treehouse is in early development: expect rough edges and changes. Feedback is welcome on
      <a href="https://github.com/paulvandermeijs/treehouse/issues">GitHub</a> or
      <a href="https://discord.com/channels/1564952399796502608">Discord</a>.
    </p>
  </div>
</template>

<style>
:root {
  --vp-layout-top-height: 40px;
}

/* Up to three lines on a phone. */
@media (max-width: 640px) {
  :root {
    --vp-layout-top-height: 64px;
  }
}

.early-banner {
  position: fixed;
  top: 0;
  right: 0;
  left: 0;
  z-index: var(--vp-z-index-layout-top);
  display: grid;
  place-items: center;
  height: var(--vp-layout-top-height);
  padding: 0 16px;
  box-sizing: border-box;
  background: var(--vp-c-brand-soft);
  border-bottom: 1px solid var(--vp-c-divider);
  color: var(--vp-c-text-1);
  font-size: 14px;
  line-height: 20px;
  text-align: center;
}

.early-banner p {
  margin: 0;
}

@media (max-width: 640px) {
  .early-banner {
    font-size: 13px;
    line-height: 18px;
  }
}

.early-banner a {
  color: var(--vp-c-brand-1);
  font-weight: 500;
  text-decoration: underline;
  text-underline-offset: 2px;
}
</style>
