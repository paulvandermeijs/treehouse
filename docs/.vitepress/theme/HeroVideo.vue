<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
import { useData, withBase } from 'vitepress'

// The app clicking through its features, in the theme the page is showing.
const { isDark } = useData()
const src = (ext: string) => withBase(`/hero-${isDark.value ? 'mocha' : 'latte'}.${ext}`)

// The page is pre-rendered in the light theme, and hydrating doesn't update
// attributes, so the video is only added in the browser, once the theme is
// known. The frame keeps its place until then.
const mounted = ref(false)
const video = ref<HTMLVideoElement>()

// Played from script rather than with autoplay, so visitors who ask for less
// motion see the poster instead.
function play() {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return
  video.value?.play().catch(() => {})
}

onMounted(() => {
  mounted.value = true
  nextTick(play)
})
watch(isDark, () => nextTick(play))
</script>

<template>
  <div class="hero-video">
    <video
      v-if="mounted"
      ref="video"
      :key="isDark ? 'mocha' : 'latte'"
      :poster="src('jpg')"
      muted
      loop
      playsinline
      preload="auto"
      aria-label="Treehouse running a Pi agent in a worktree: approving a command, a sub-agent and the diff"
    >
      <source :src="src('webm')" type="video/webm" />
      <source :src="src('mp4')" type="video/mp4" />
    </video>
  </div>
</template>
