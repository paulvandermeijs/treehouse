import { h } from 'vue'
import DefaultTheme from 'vitepress/theme'
import EarlyBanner from './EarlyBanner.vue'
import HeroVideo from './HeroVideo.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'layout-top': () => h(EarlyBanner),
      'home-hero-image': () => h(HeroVideo),
    }),
}
