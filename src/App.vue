<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from './lib/api';
import QuickPanel from './views/QuickPanel.vue';
import Manager from './views/Manager.vue';
import CaptureView from './components/CaptureView.vue';

const label = getCurrentWindow().label;
const isManager = computed(() => label === 'manager');
const isCapture = computed(() => label === 'capture');
// 给 body 打窗口标记：不同窗口的 webview 画布底色不同（如快捷面板需与面板同色，
// 否则自绘圆角与系统 DWM 圆角之间会露出异色楔形），见 base.css body[data-window]
document.body.dataset.window = label;

async function applyTheme() {
  try {
    const d = await api.getData();
    const t = d.settings.theme;
    document.documentElement.dataset.theme =
      t === 'light' || t === 'auto' ? t : 'dark';
  } catch {
    document.documentElement.dataset.theme = 'dark';
  }
}

let unlisten: (() => void) | undefined;
onMounted(async () => {
  await applyTheme();
  unlisten = await listen('data-changed', applyTheme);
});
onBeforeUnmount(() => unlisten?.());
</script>

<template>
  <CaptureView v-if="isCapture" />
  <Manager v-else-if="isManager" />
  <QuickPanel v-else />
</template>
