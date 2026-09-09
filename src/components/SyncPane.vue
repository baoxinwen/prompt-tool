<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, onUnmounted, ref, watch } from 'vue';
import { Cloud, RefreshCw, UploadCloud, DownloadCloud, Eye, EyeOff } from 'lucide-vue-next';
import { api } from '../lib/api';
import { managerKey } from '../lib/context';
import { relativeTime } from '../lib/relativeTime';
import AccentButton from './ui/AccentButton.vue';
import Segmented from './ui/Segmented.vue';

const ctx = inject(managerKey)!;

const provider = ref<'webdav' | 'gist'>('webdav');

// WebDAV 表单
const url = ref('');
const username = ref('');
const password = ref('');
const davEnabled = ref(false);
const davAutoSync = ref(false);

// GitHub Gist 表单
const token = ref('');
const gistId = ref('');
const gistEnabled = ref(false);
const gistAutoSync = ref(false);

const showWebdavPwd = ref(false);
const showToken = ref(false);
const testing = ref(false);
const syncing = ref(false);
const status = ref('');
const statusOk = ref(true);
const formDirty = ref(false);
const syncClipboard = ref(false);

const providerOptions = [
  { id: 'webdav', label: 'WebDAV' },
  { id: 'gist', label: 'GitHub Gist' },
];

// ---------- 相对时间：上次同步 + 动作完成回填（30s 定时刷新） ----------
const nowTick = ref(Date.now());
let clock: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  clock = setInterval(() => {
    nowTick.value = Date.now();
  }, 30_000);
});
onUnmounted(() => clearInterval(clock));

function relTime(ts: number) {
  return relativeTime(ts, new Date(nowTick.value));
}

const lastSyncLabel = computed(() => {
  const ts = ctx.data.value?.settings.lastSyncAt;
  return ts ? `上次同步 · ${relTime(ts)}` : '尚未同步';
});

const statsLabel = computed(() => {
  const d = ctx.data.value;
  return `${d?.prompts?.length ?? 0} 条提示词 · ${d?.clipboard?.length ?? 0} 条剪贴板`;
});

// ---------- 状态三态：未配置 faint / 已配置·未启用 warn / 已启用 ok ----------
const providerName = computed(() => (provider.value === 'gist' ? 'GitHub Gist' : 'WebDAV'));
const providerOn = computed(() =>
  provider.value === 'gist' ? gistEnabled.value : davEnabled.value,
);
// 门槛 = 当前 provider 的必填项非空：webdav → 服务器地址，gist → Token
const configured = computed(() =>
  provider.value === 'gist' ? token.value.trim() !== '' : url.value.trim() !== '',
);
const state = computed<'on' | 'ready' | 'unset'>(() =>
  providerOn.value ? 'on' : configured.value ? 'ready' : 'unset',
);
const stateLabel = computed(() =>
  state.value === 'on' ? '已启用' : state.value === 'ready' ? '已配置·未启用' : '未配置',
);

// 启用/自动同步跟随 provider：同一行开关读写不同后端的配置
const providerEnabled = computed({
  get: () => (provider.value === 'gist' ? gistEnabled.value : davEnabled.value),
  set: (v: boolean) => {
    if (provider.value === 'gist') gistEnabled.value = v;
    else davEnabled.value = v;
    markDirty();
  },
});
const providerAutoSync = computed({
  get: () => (provider.value === 'gist' ? gistAutoSync.value : davAutoSync.value),
  set: (v: boolean) => {
    if (provider.value === 'gist') gistAutoSync.value = v;
    else davAutoSync.value = v;
    markDirty();
  },
});

// 用户是否手动编辑过 Gist ID 输入框：未编辑时该字段始终跟随后端，
// 防止保存时把后端自动回填的 gist_id 覆盖回空串（评审 I12）
const gistIdDirty = ref(false);

function markDirty() {
  formDirty.value = true;
}

function onGistIdInput() {
  markDirty();
  gistIdDirty.value = true;
}

function fillFrom(s: import('../types').Settings) {
  provider.value = s.syncProvider === 'gist' ? 'gist' : 'webdav';
  url.value = s.webdav.url;
  username.value = s.webdav.username;
  password.value = s.webdav.password;
  davEnabled.value = s.webdav.enabled;
  davAutoSync.value = s.webdav.autoSync;
  token.value = s.gist.token;
  gistId.value = s.gist.gistId;
  gistEnabled.value = s.gist.enabled;
  gistAutoSync.value = s.gist.autoSync;
  syncClipboard.value = s.syncClipboard ?? false;
  gistIdDirty.value = false;
  formDirty.value = false;
}

watch(
  ctx.data,
  (d) => {
    if (d && !formDirty.value) fillFrom(d.settings);
  },
  { immediate: true },
);

// 后端首次同步自动创建 Gist 后，会把 id 写回 settings 并推 data-changed。
// 表单脏时上面的 fillFrom 被跳过，但 gistId 是服务端分配值必须始终跟随后端，
// 否则用户随后保存会用表单里的空串覆盖后端 id，导致重复创建 Gist（评审 I12）
watch(ctx.data, (d) => {
  if (d && !gistIdDirty.value) gistId.value = d.settings.gist.gistId;
});

// 表单有未保存修改时不允许被切换标签静默丢弃（评审 I9），守卫由 Manager.switchTab 消费
onMounted(() => ctx.setLeaveGuard(() => !formDirty.value));
onBeforeUnmount(() => ctx.setLeaveGuard(null));

function switchProvider(p: string) {
  provider.value = p as 'webdav' | 'gist';
  formDirty.value = true;
}

// ---------- 状态行：动作完成后回填「✓ … · 刚刚」 ----------
const statusAt = ref(0);
function setStatus(msg: string, ok: boolean) {
  status.value = msg;
  statusOk.value = ok;
  statusAt.value = Date.now();
}

async function save(silent = false): Promise<boolean> {
  const s = ctx.data.value?.settings;
  if (!s) return false;
  try {
    await api.saveSettings({
      ...s,
      syncProvider: provider.value,
      syncClipboard: syncClipboard.value,
      webdav: {
        enabled: davEnabled.value,
        autoSync: davAutoSync.value,
        url: url.value.trim(),
        username: username.value.trim(),
        password: password.value,
      },
      gist: {
        enabled: gistEnabled.value,
        autoSync: gistAutoSync.value,
        token: token.value.trim(),
        gistId: gistId.value.trim(),
      },
    });
    formDirty.value = false;
    await ctx.refresh();
    if (!silent) ctx.toast('同步配置已保存');
    return true;
  } catch (e) {
    ctx.toast(String(e), 'err');
    return false;
  }
}

async function test() {
  testing.value = true;
  status.value = '';
  try {
    const msg =
      provider.value === 'gist'
        ? await api.gistTest(token.value.trim(), gistId.value.trim())
        : await api.webdavTest(url.value.trim(), username.value.trim(), password.value);
    setStatus(`✓ ${msg}`, true);
  } catch (e) {
    setStatus(`✗ ${e}`, false);
  } finally {
    testing.value = false;
  }
}

async function doSync(direction: 'merge' | 'push' | 'pull') {
  // 门槛：未配置不得同步（按钮同时禁用，双保险）
  if (!configured.value) return;
  // push/pull 是整体覆盖的破坏性操作，必须先确认
  if (direction === 'push') {
    const ok = await ctx.confirm({
      title: '仅上传：用本机数据覆盖云端？',
      message: '云端现有的提示词与剪贴板历史将被本机数据整体替换，云端被覆盖的内容无法找回。',
      confirmText: '覆盖云端',
      danger: true,
    });
    if (!ok) return;
  } else if (direction === 'pull') {
    const ok = await ctx.confirm({
      title: '仅下载：用云端数据覆盖本机？',
      message:
        '本机现有的提示词与文本剪贴板历史将被云端数据整体替换（本机图片剪贴板会保留），覆盖前的本机内容无法找回。',
      confirmText: '覆盖本机',
      danger: true,
    });
    if (!ok) return;
  }
  syncing.value = true;
  status.value = '';
  try {
    if (!(await save(true))) return;
    const report = await api.syncNow(direction);
    setStatus(`✓ ${report.message}`, true);
    await ctx.refresh();
  } catch (e) {
    setStatus(`✗ ${e}`, false);
  } finally {
    syncing.value = false;
  }
}

// 「去填写」：把焦点带到当前 provider 的必填输入框（地址 / Token）
const urlEl = ref<HTMLInputElement | null>(null);
const tokenEl = ref<HTMLInputElement | null>(null);
function goFill() {
  (provider.value === 'gist' ? tokenEl : urlEl).value?.focus();
}

const httpWarn = () => /^http:\/\//i.test(url.value.trim());
</script>

<template>
  <div class="sync">
    <header class="sync-head">
      <h2 class="sync-title">云同步</h2>
      <span class="on-badge" :class="{ on: state === 'on', ready: state === 'ready' }">
        <span class="on-dot" />
        {{ stateLabel }}
      </span>
      <span class="grow" />
    </header>

    <div class="sync-body">
      <!-- v3：整页一张卡，左侧 3px 状态饰条 -->
      <div class="onecard card">
        <!-- 后端切换：单卡第一行——provider 决定下方状态/开关/表单 -->
        <Segmented
          :model-value="provider"
          :options="providerOptions"
          @update:model-value="switchProvider"
        />

        <!-- 状态区：icon + provider + 徽章 + 描述 + 上次同步/条目统计 + 同步按钮 -->
        <div class="overview">
          <div class="ov-icon">
            <Cloud :size="21" :stroke-width="1.8" />
          </div>
          <div class="ov-info">
            <div class="ov-title-row">
              <span class="ov-title">{{ providerName }}</span>
              <span class="state-badge" :class="`st-${state}`">{{ stateLabel }}</span>
            </div>
            <div class="ov-desc muted">条目级合并：同条目保留较新版本，删除也会同步到其他设备</div>
            <div class="ov-meta muted tnum">
              <span>{{ lastSyncLabel }}</span>
              <span class="meta-sep">·</span>
              <span>{{ statsLabel }}</span>
              <span class="meta-sep">·</span>
              <span>图片仅存本机，不参与同步</span>
            </div>
          </div>
          <div class="ov-actions">
            <button class="ob" :disabled="syncing || !configured" title="双向合并" @click="doSync('merge')">
              <RefreshCw :size="14" :class="{ spin: syncing }" /> 立即同步
            </button>
            <button class="ob" :disabled="syncing || !configured" title="本机覆盖云端" @click="doSync('push')">
              <UploadCloud :size="14" /> 仅上传
            </button>
            <button class="ob" :disabled="syncing || !configured" title="云端覆盖本机" @click="doSync('pull')">
              <DownloadCloud :size="14" /> 仅下载
            </button>
            <button v-if="!configured" class="ob go-fill" title="填好必填项后即可同步" @click="goFill">
              去填写
            </button>
          </div>
        </div>

        <!-- 开关区：启用 / 自动同步（跟随 provider）+ 同步范围 -->
        <div class="opts">
          <label class="row opt">
            <span class="switch">
              <input v-model="providerEnabled" type="checkbox" />
              <span class="track"><span class="thumb" /></span>
            </span>
            <span>
              启用 {{ providerName }} 同步
              <small class="muted block-note">只影响自动同步，仍可手动同步</small>
            </span>
          </label>
          <label class="row opt">
            <span class="switch">
              <input v-model="providerAutoSync" type="checkbox" />
              <span class="track"><span class="thumb" /></span>
            </span>
            <span>
              自动同步
              <small class="muted block-note">启动时与内容变更后自动合并</small>
            </span>
          </label>
          <label class="row opt">
            <span class="switch">
              <input v-model="syncClipboard" type="checkbox" @change="markDirty" />
              <span class="track"><span class="thumb" /></span>
            </span>
            <span>
              云同步包含剪贴板历史
              <small class="muted block-note">敏感内容也会上云，多设备需保持一致</small>
            </span>
          </label>
        </div>

        <div class="hairline" />

        <!-- 表单字段（跟随上方 provider 选择） -->
        <!-- WebDAV 表单 -->
        <div v-if="provider === 'webdav'" class="form">
          <p class="muted hint">
            推荐坚果云：<span class="mono">https://dav.jianguoyun.com/dav/prompt-tool/</span>
            （末级目录自动创建；密码使用应用密码）
          </p>
          <label class="field">
            <span>服务器地址</span>
            <input
              ref="urlEl"
              v-model="url"
              type="text"
              placeholder="https://dav.jianguoyun.com/dav/prompt-tool/"
              spellcheck="false"
              @input="markDirty"
            />
          </label>
          <div class="row-fields">
            <label class="field grow">
              <span>账号</span>
              <input v-model="username" type="text" autocomplete="off" spellcheck="false" @input="markDirty" />
            </label>
            <label class="field grow">
              <span>密码 / 应用密码</span>
              <span class="pwd-box">
                <input
                  v-model="password"
                  :type="showWebdavPwd ? 'text' : 'password'"
                  autocomplete="new-password"
                  @input="markDirty"
                />
                <button
                  class="eye"
                  type="button"
                  :aria-label="showWebdavPwd ? '隐藏密码' : '显示密码'"
                  @click="showWebdavPwd = !showWebdavPwd"
                >
                  <Eye v-if="showWebdavPwd" :size="14" />
                  <EyeOff v-else :size="14" />
                </button>
              </span>
            </label>
          </div>
          <p v-if="httpWarn()" class="muted hint warn-hint">
            当前使用 http:// 连接，账号密码将以明文传输；如服务器支持，建议改用 https://
          </p>
        </div>

        <!-- GitHub Gist 表单 -->
        <div v-else class="form">
          <p class="muted hint">
            数据保存在你的 <b>secret Gist</b>（不公开、仅凭 Token 可访问、自带版本历史）。
            Token 创建：GitHub → Settings → Developer settings →
            <b>Personal access tokens (classic)</b> → 勾选 <span class="mono">gist</span> 权限。
          </p>
          <label class="field">
            <span>GitHub Token</span>
            <span class="pwd-box">
              <input
                ref="tokenEl"
                v-model="token"
                :type="showToken ? 'text' : 'password'"
                placeholder="ghp_… / github_pat_…"
                autocomplete="new-password"
                spellcheck="false"
                @input="markDirty"
              />
              <button
                class="eye"
                type="button"
                :aria-label="showToken ? '隐藏 Token' : '显示 Token'"
                @click="showToken = !showToken"
              >
                <Eye v-if="showToken" :size="14" />
                <EyeOff v-else :size="14" />
              </button>
            </span>
          </label>
          <label class="field">
            <span>Gist ID（留空则首次同步时自动创建）</span>
            <input
              v-model="gistId"
              type="text"
              placeholder="自动创建后回填显示"
              spellcheck="false"
              @input="onGistIdInput"
            />
          </label>
        </div>

        <!-- 卡底操作行 -->
        <div class="btns">
          <button :disabled="testing" @click="test">{{ testing ? '测试中…' : '测试连接' }}</button>
          <AccentButton :disabled="testing || syncing" @click="save()">保存配置</AccentButton>
        </div>

        <!-- 终端式状态条 -->
        <div v-if="status" class="status tnum" :class="{ ok: statusOk, err: !statusOk }">
          <span class="status-prompt mono">pm</span>
          {{ status }}<template v-if="statusAt"> · {{ relTime(statusAt) }}</template>
        </div>

        <div class="note faint">
          「立即同步 / 仅上传 / 仅下载」会先自动保存当前表单配置。
          凭据仅保存在本机的系统凭据管理器（Windows 凭据管理器），不会随数据上传。
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sync {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.sync-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 18px;
  height: 52px;
  flex: none;
  border-bottom: 1px solid var(--border);
}

.sync-title {
  font-size: var(--fs-lg);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.on-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--faint);
  background: var(--panel-2);
  border: 1px solid var(--border);
  padding: 2.5px 10px;
  border-radius: 999px;
}

.on-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--faint);
}

.on-badge.ready {
  color: var(--warn);
  background: var(--warn-soft);
  border-color: transparent;
}

.on-badge.ready .on-dot {
  background: var(--warn);
}

.on-badge.on {
  color: var(--ok);
  background: var(--ok-soft);
  border-color: transparent;
}

.on-badge.on .on-dot {
  background: var(--ok);
  box-shadow: 0 0 6px var(--ok);
}

.sync-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 18px 22px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 700px;
  width: 100%;
  /* 限宽列水平居中，与设置/数据页一致 */
  margin: 0 auto;
}

/* 滚动容器内的块级子元素不参与高度收缩：否则内容溢出时卡片先被 flex
   压扁（最小高度为 0）而非出现滚动条 */
.sync-body > * {
  flex: none;
}

/* ---------- 单卡（v3）：左侧 3px 状态饰条 ---------- */

.onecard {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px 18px;
}

/* 状态区 */

.overview {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 14px;
  row-gap: 10px;
}

.ov-icon {
  width: 44px;
  height: 44px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  background: var(--brand-soft);
  color: var(--brand);
  flex: none;
}

.ov-info {
  /* basis=auto + 最小宽度：空间不足时按钮整行换行（flex-wrap），文本保持单行 */
  flex: 1 1 auto;
  min-width: 220px;
}

.ov-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.ov-title {
  font-weight: 650;
  font-size: var(--fs-lg);
}

.state-badge {
  font-size: 11px;
  padding: 2px 9px;
  border-radius: 999px;
  border: 1px solid transparent;
  color: var(--faint);
  background: var(--panel-2);
}

.state-badge.st-ready {
  color: var(--warn);
  background: var(--warn-soft);
}

.state-badge.st-on {
  color: var(--ok);
  background: var(--ok-soft);
}

.ov-desc {
  font-size: 12px;
  margin-top: 3px;
}

.ov-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  margin-top: 5px;
}

.meta-sep {
  color: var(--faint);
}

.ov-actions {
  display: flex;
  gap: 7px;
  flex: 1 1 100%;
  /* 独占一行：空间再窄也保持在状态信息下方，不与之争宽 */
}

.ob {
  font-size: 12px;
  padding: 7px 13px;
  gap: 6px;
}

.go-fill {
  color: var(--warn);
  border-color: var(--warn);
  background: var(--warn-soft);
}

.spin {
  animation: spin 900ms linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* 后端切换：卡首紧凑胶囊（不随 flex 列拉伸成通栏灰带） */
.onecard > .seg {
  align-self: flex-start;
}

/* 开关区 */

.opts {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.hairline {
  height: 1px;
  background: var(--border);
  flex: none;
}

/* 表单区 */

.form {
  display: flex;
  flex-direction: column;
  gap: 13px;
}

.hint {
  font-size: 12px;
  line-height: 1.7;
  padding: 9px 12px;
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
}

.mono {
  font-family: var(--font-mono);
  color: var(--brand);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.pwd-box {
  position: relative;
  display: flex;
  align-items: center;
}

.pwd-box input {
  width: 100%;
  padding-right: 34px;
}

.pwd-box .eye {
  position: absolute;
  right: 4px;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--muted);
}

.field > span {
  font-size: 12px;
  color: var(--muted);
}

.row-fields {
  display: flex;
  gap: 10px;
}

.opt {
  gap: 10px;
  font-size: 12.5px;
  cursor: pointer;
}

.block-note {
  display: block;
  margin-top: 3px;
  line-height: 1.6;
  max-width: 520px;
}

.warn-hint {
  color: var(--warn);
  border-color: var(--warn);
}

.btns {
  display: flex;
  gap: 8px;
  align-items: center;
}

.status {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 12px;
  border-radius: var(--r-sm);
  font-size: 12.5px;
  border: 1px solid;
}

.status-prompt {
  font-size: 10.5px;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--panel-3);
  color: var(--muted);
}

.status.ok {
  border-color: var(--ok);
  color: var(--ok);
  background: var(--ok-soft);
}

.status.err {
  border-color: var(--danger-btn);
  color: var(--danger);
  background: var(--danger-soft);
}

.note {
  font-size: 11.5px;
  line-height: 1.7;
  padding-bottom: 8px;
}
</style>
