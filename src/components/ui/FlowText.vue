<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';

/**
 * 流式打字（铁律 4）：逐字从 blur(6px) 到清晰，单字 90ms，
 * 缓动 cubic-bezier(.215,.61,.355,1)，相邻字错开 30ms。
 * 长文案的错开步进封顶（默认 8 步），整体不至于久等；
 * prefers-reduced-motion 下直接静态呈现。
 */
const props = withDefaults(
  defineProps<{
    text: string;
    /** 错开步进封顶步数（每步 30ms） */
    maxSteps?: number;
  }>(),
  { maxSteps: 8 },
);

const play = ref(false);
onMounted(() => {
  requestAnimationFrame(() => (play.value = true));
});

const chars = computed(() => Array.from(props.text));

const reduceMotion =
  typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
const animate = computed(() => play.value && !reduceMotion);
</script>

<template>
  <!-- 逐字拆分供 CSS 过渡，aria-label 保整句朗读 -->
  <span class="flow-text" role="text" :aria-label="text">
    <span
      v-for="(ch, i) in chars"
      :key="i"
      aria-hidden="true"
      class="ft-ch"
      :class="{ play: animate }"
      :style="{ transitionDelay: `${Math.min(i, maxSteps) * 30}ms` }"
    >{{ ch }}</span>
  </span>
</template>

<style scoped>
.flow-text {
  white-space: pre-wrap;
}

.ft-ch {
  display: inline-block;
  opacity: 0;
  filter: blur(6px);
  transition:
    opacity 90ms cubic-bezier(0.215, 0.61, 0.355, 1),
    filter 90ms cubic-bezier(0.215, 0.61, 0.355, 1);
}

.ft-ch.play {
  opacity: 1;
  filter: blur(0);
}

@media (prefers-reduced-motion: reduce) {
  .ft-ch {
    opacity: 1;
    filter: none;
    transition: none;
  }
}
</style>
