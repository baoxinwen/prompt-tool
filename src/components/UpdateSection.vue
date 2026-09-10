<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted } from 'vue';
import { RefreshCw } from 'lucide-vue-next';
import { api } from '../lib/api';
import {
  lastAutoUpdate,
  updatePhase,
  updateInfo,
  updateErrMsg,
  updateErrorKind,
  updateProgress,
} from '../lib/updateCache';
import { enqueueSettingsSave } from '../lib/settingsSave';
import { managerKey } from '../lib/context';
import type { Settings, UpdateProgress } from '../types';

const ctx = inject(managerKey)!;
// 由 vite define 注入，与 package.json / tauri.conf.json 同源
const appVersion = __APP_VERSION__;

// 状态机在 updateCache.ts 模块级（评审 2026-09-10 I7）：下载中切页卸载本组件
// 后重挂载，phase/进度仍在，不会误判空闲而二次触发下载
const phase = updatePhase;
const info = updateInfo;
const errMsg = updateErrMsg;
const errorKind = updateErrorKind;
const progress = updateProgress;
let unlisten: (() => void) | null = null;

const ERROR_TEXT: Record<string, string> = {
  network: '网络不通：无法连接 GitHub 与镜像源，可稍后重试或去发布页手动下载',
  signature: '更新包签名校验失败：为安全起见已中止，请去发布页手动下载',
  unknown: '更新失败',
};

function onProgress(p: UpdateProgress) {
  progress.value = p;
  if (p.percent !== null && p.percent >= 100) phase.value = 'installing';
}

/** 下载/安装中重挂载（切页回来）：重新订阅进度事件，恢复进度显示 */
async function resubscribeProgress() {
  try {
    unlisten?.();
    unlisten = await api.onUpdateProgress(onProgress);
  } catch {
    /* 订阅失败保留已有进度显示，下载本身不受影响 */
  }
}

async function check() {
  // 下载/安装进行中不允许重新检查：避免状态机被拉回、旧 unlisten 泄漏
  if (phase.value === 'checking' || phase.value === 'downloading' || phase.value === 'installing') return;
  phase.value = 'checking';
  try {
    const st = await api.checkUpdate();
    if (st.kind === 'available') {
      info.value = st;
      phase.value = 'available';
    } else if (st.kind === 'upToDate') {
      phase.value = 'uptodate';
    } else {
      errMsg.value = st.message;
      errorKind.value = st.errorKind;
      phase.value = 'error';
    }
  } catch (e) {
    errMsg.value = String(e);
    errorKind.value = 'unknown';
    phase.value = 'error';
  }
}

async function install() {
  // phase 在模块级：即使本组件曾被卸载重挂，下载中的 phase 也拦得住二次触发（I7）
  if (!info.value || phase.value === 'downloading' || phase.value === 'installing') return;
  phase.value = 'downloading';
  progress.value = null;
  try {
    // 订阅必须在 try 内：listen 失败若悬在 try 外会成为未处理 rejection，且 phase 卡死 downloading
    unlisten = await api.onUpdateProgress(onProgress);
    await api.downloadAndInstallUpdate();
    // 正常情况下进程重启、走不到这里；走到这里说明安装器已交管，按安装中处理
    phase.value = 'installing';
  } catch (e) {
    errMsg.value = String(e);
    errorKind.value = 'unknown';
    phase.value = 'error';
  } finally {
    unlisten?.();
    unlisten = null;
  }
}

async function skip() {
  const available = info.value;
  const s = ctx.data.value?.settings as Settings | undefined;
  if (!available || !s) return;
  // 与 SettingsPane 共用串行化队列：避免与其它设置保存交叠时互相回滚（评审 I3）
  await enqueueSettingsSave(async () => {
    const next = JSON.parse(JSON.stringify(ctx.data.value?.settings)) as Settings | undefined;
    if (!next) return;
    next.skippedUpdateVersion = available.version;
    try {
      await api.saveSettings(next);
      await ctx.refresh();
      ctx.toast(`已跳过 v${available.version}，设置页仍可随时更新`);
    } catch (e) {
      ctx.toast(String(e), 'err');
    }
  });
}

const openReleases = () => api.openReleasesPage().catch((e) => ctx.toast(String(e), 'err'));

/** total 未知（服务器未回 Content-Length）时 percent 为 null，
 *  只报已下载量，避免误导性的「下载中 0%」（Task 0 实证会发生） */
const progressText = computed(() => {
  const p = progress.value;
  if (p && p.percent !== null) return `下载中 ${p.percent}%`;
  return `已下载 ${((p?.downloaded ?? 0) / 1048576).toFixed(1)} MB`;
});

// 消费启动自动检查的结果（F3）：有新版时直接进入 available，点「去更新」后无需二次手动检查。
// 不做 skip 过滤——设置页常驻展示不受跳过影响（F8 语义，skip 过滤在 Manager 写入侧完成）
onMounted(() => {
  const cached = lastAutoUpdate.value;
  if (cached?.kind !== 'available') {
    // idle 之外可能是上次卸载时仍在下载/安装：重挂载后恢复进度订阅（I7）
    if (phase.value === 'downloading' || phase.value === 'installing') {
      void resubscribeProgress();
    }
    return;
  }
  info.value = cached;
  phase.value = 'available';
  lastAutoUpdate.value = null; // 一次性消费
});

onBeforeUnmount(() => { unlisten?.(); unlisten = null; });
</script>

<template>
  <div class="upd">
    <p class="upd-now">
      当前版本 <strong>v{{ appVersion }}</strong>
      <span v-if="phase === 'uptodate'" class="faint">· 已是最新</span>
    </p>

    <!-- available 之后（含下载/安装中）保持展示新版本信息，操作仅在 available 可点 -->
    <div
      v-if="info && (phase === 'available' || phase === 'downloading' || phase === 'installing')"
      class="upd-avail"
    >
      <p>发现新版本 <strong>v{{ info.version }}</strong></p>
      <pre class="upd-notes">{{ info.notes || '详见发布页' }}</pre>
      <div class="upd-actions">
        <button class="primary install" :disabled="phase !== 'available'" @click="install">立即更新</button>
        <button class="skip" :disabled="phase !== 'available'" @click="skip">跳过此版本</button>
        <button class="releases" @click="openReleases">去发布页</button>
      </div>
    </div>

    <div v-if="phase === 'downloading'" class="upd-progress">
      <div
        class="progress"
        role="progressbar"
        :aria-valuenow="progress?.percent ?? undefined"
        aria-valuemin="0"
        aria-valuemax="100"
      ><span :style="{ width: (progress?.percent ?? 0) + '%' }" /></div>
      <small class="faint">{{ progressText }}</small>
    </div>

    <p v-if="phase === 'installing'" class="faint">安装中，应用即将重启…</p>

    <div v-if="phase === 'error'" class="upd-err">
      <p>{{ ERROR_TEXT[errorKind] || '检查更新失败' }}</p>
      <p v-if="errMsg" class="faint">{{ errMsg }}</p>
      <div class="upd-actions">
        <button class="retry" @click="check">重试</button>
        <button class="releases" @click="openReleases">去发布页</button>
      </div>
    </div>

    <button
      class="check"
      :disabled="phase === 'checking' || phase === 'downloading' || phase === 'installing'"
      @click="check"
    >
      <RefreshCw :size="13" /> {{ phase === 'checking' ? '检查中…' : '检查更新' }}
    </button>
  </div>
</template>

<style scoped>
.upd {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.upd-now {
  font-size: var(--fs-base);
}

.upd-avail {
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  padding: 12px 14px;
}

.upd-avail > p {
  font-size: var(--fs-base);
}

.upd-notes {
  font-family: var(--font-mono);
  font-size: var(--fs-sm);
  line-height: 1.6;
  color: var(--text-2);
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: var(--r-xs);
  padding: 8px 10px;
  margin: 8px 0 10px;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 120px;
  overflow-y: auto;
}

.upd-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.upd-progress {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.progress {
  height: 6px;
  border-radius: 3px;
  background: var(--panel-3);
  overflow: hidden;
}

.progress span {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--brand-btn);
  transition: width var(--t-med);
}

.upd-err {
  background: var(--warn-soft);
  border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--border));
  border-radius: var(--r-sm);
  padding: 10px 14px;
  font-size: var(--fs-sm);
  color: var(--text-2);
}

.upd-err .upd-actions {
  margin-top: 8px;
}

.check {
  align-self: flex-start;
}
</style>
