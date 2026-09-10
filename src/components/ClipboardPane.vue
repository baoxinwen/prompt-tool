<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref } from 'vue';
import {
  Copy,
  Trash2,
  Search,
  SearchX,
  ClipboardList,
  Type as TypeIcon,
  Image as ImageIcon,
} from 'lucide-vue-next';
import { api } from '../lib/api';
import { managerKey } from '../lib/context';
import { filterClipboard, formatTime } from '../lib/search';
import { TIME_GROUPS, timeGroup, groupTimeLabel, type TimeGroupName } from '../lib/timeGroup';
import { useImageThumbs } from '../lib/thumbs';
import EmptyState from './ui/EmptyState.vue';
import type { ClipboardItem } from '../types';

const ctx = inject(managerKey)!;
const query = ref('');
const searchInput = ref<HTMLInputElement | null>(null);

const MAX_RENDER = 200;
const totalCount = computed(() => (ctx.data.value?.clipboard ?? []).length);
const searching = computed(() => query.value.trim().length > 0);
const allItems = computed(() => filterClipboard(ctx.data.value?.clipboard ?? [], query.value));
const items = computed(() =>
  searching.value ? allItems.value : allItems.value.slice(0, MAX_RENDER),
);
const hiddenCount = computed(() => allItems.value.length - items.value.length);

/** 时间分组：按 今天/昨天/本周/更早 桶装，只保留非空组（顺序沿 TIME_GROUPS） */
const groups = computed(() => {
  const buckets = new Map<TimeGroupName, ClipboardItem[]>(TIME_GROUPS.map((g) => [g, []]));
  const now = new Date();
  for (const it of items.value) {
    buckets.get(timeGroup(it.copiedAt, now))!.push(it);
  }
  return TIME_GROUPS.filter((g) => buckets.get(g)!.length > 0).map((name) => ({
    name,
    items: buckets.get(name)!,
  }));
});

const atTop = ref(true);
const atBottom = ref(true);

function updateEdges(e: Event) {
  const el = e.target as HTMLElement;
  atTop.value = el.scrollTop < 8;
  atBottom.value = el.scrollTop + el.clientHeight >= el.scrollHeight - 8;
}

/** 图片缩略图惰性加载（与 QuickPanel 共用实现，评审 M9） */
const { thumbFor } = useImageThumbs();

/** 全文浮层：点击行展开阅读，Esc / ← / 遮罩关闭 */
const detailItem = ref<ClipboardItem | null>(null);

function openDetail(c: ClipboardItem) {
  detailItem.value = c;
}

function closeDetail() {
  detailItem.value = null;
}

async function copy(item: ClipboardItem) {
  try {
    if (item.kind === 'image') {
      await api.copyImage(item.id);
      ctx.toast('图片已复制到剪贴板');
    } else {
      await api.copyText(item.content);
      ctx.toast('已复制');
    }
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

async function removeItem(id: string) {
  try {
    await api.deleteHistoryItem(id);
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

async function clearAll() {
  // 用未过滤的全量条数：clear_history 后端全量清空，确认框数字必须与之相符，
  // 不能用 allItems（搜索过滤后）或 items（渲染截断）的长度
  if (!totalCount.value) return;
  const ok = await ctx.confirm({
    title: `清空全部 ${totalCount.value} 条剪贴板历史？`,
    message: '清空后可在 5 秒内撤销。',
    confirmText: '清空',
    danger: true,
  });
  if (!ok) return;
  try {
    const n = await api.clearHistory();
    await ctx.refresh();
    // 撤销窗口由 Manager 的 toast 机制限时（带 action 默认 5s）
    ctx.toast(`已清空 ${n} 条`, 'ok', {
      label: '撤销',
      handler: async () => {
        try {
          await api.restoreHistory();
          await ctx.refresh();
        } catch (e) {
          ctx.toast(String(e), 'err');
        }
      },
    });
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

async function toggleCapture() {
  const s = ctx.data.value?.settings;
  if (!s) return;
  try {
    await api.saveSettings({ ...s, captureClipboard: !s.captureClipboard });
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

// Ctrl+K 聚焦搜索；浮层打开时 Esc / ← 关闭
function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    // 详情浮层打开时不抢焦点（评审 2026-09-10 M4#5）：焦点进入被
    // 遮罩挡住的搜索框会让用户键入"消失"，表现为键盘失灵
    if (detailItem.value) return;
    e.preventDefault();
    searchInput.value?.focus();
    return;
  }
  if (detailItem.value && (e.key === 'Escape' || e.key === 'ArrowLeft')) {
    e.preventDefault();
    closeDetail();
  }
}

function focusSearch() {
  searchInput.value?.focus();
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown);
  window.addEventListener('pm-focus-search', focusSearch);
});
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
  window.removeEventListener('pm-focus-search', focusSearch);
});
</script>

<template>
  <div class="cb">
    <header class="cb-head">
      <h2 class="cb-title">
        剪贴板
        <span class="cb-count tnum">{{ totalCount }}</span>
      </h2>
      <span class="grow" />
      <label class="row cap" title="关闭后将不再记录系统复制的内容">
        <span class="muted">记录新内容</span>
        <span class="switch">
          <input
            type="checkbox"
            :checked="ctx.data.value?.settings.captureClipboard"
            @change="toggleCapture"
          />
          <span class="track"><span class="thumb" /></span>
        </span>
      </label>
      <div class="search-box">
        <Search :size="14" class="search-ico" />
        <input
          ref="searchInput"
          v-model="query"
          type="text"
          placeholder="搜索剪贴板…"
          spellcheck="false"
        />
        <kbd class="search-hint">Ctrl K</kbd>
      </div>
      <span class="head-sep" aria-hidden="true" />
      <button class="ghost-btn danger clear-btn" :disabled="!totalCount" @click="clearAll">
        <Trash2 :size="13" /> 清空
      </button>
    </header>

    <div
      class="list"
      :class="{ 'fade-top': !atTop, 'fade-bottom': !atBottom }"
      @scroll="updateEdges"
    >
      <div v-if="searching" class="hitline tnum">找到 {{ allItems.length }} 条</div>
      <template v-for="g in groups" :key="g.name">
        <div class="grp">{{ g.name }}</div>
        <div
          v-for="c in g.items"
          :key="c.id"
          class="row-item"
          :class="{ img: c.kind === 'image' }"
          @click="openDetail(c)"
        >
          <span class="kind-ico" aria-hidden="true">
            <ImageIcon v-if="c.kind === 'image'" :size="13" />
            <TypeIcon v-else :size="13" />
          </span>
          <img
            v-if="c.kind === 'image' && c.image"
            :src="thumbFor(c)"
            class="clip-img"
            alt="剪贴板图片"
          />
          <div v-else class="content">{{ c.content }}</div>
          <div class="row-side">
            <span class="time tnum">{{ groupTimeLabel(c.copiedAt) }}</span>
            <span class="ops" @click.stop>
              <button class="ghost-btn sm copy-btn" title="复制" @click="copy(c)">
                <Copy :size="12" />
              </button>
              <button class="ghost-btn sm danger" title="删除" @click="removeItem(c.id)">
                <Trash2 :size="12" />
              </button>
            </span>
          </div>
        </div>
      </template>
      <div v-if="hiddenCount > 0" class="truncated muted">
        还有 {{ hiddenCount }} 条未显示，输入关键词继续筛选
      </div>
      <EmptyState v-if="searching && !allItems.length" :icon="SearchX" title="没有匹配的记录">
        换个关键词试试
      </EmptyState>
      <EmptyState v-else-if="!searching && !totalCount" :icon="ClipboardList" title="暂无记录">
        在任意程序里复制的文本和截图都会出现在这里
      </EmptyState>
    </div>

    <!-- 全文浮层：居中阅读卡（与 QuickPanel 的 detail 同一交互：Esc/←/遮罩关闭） -->
    <Transition name="detail">
      <div v-if="detailItem" class="detail-mask" @click.self="closeDetail">
        <div class="detail">
          <div class="detail-title">
            {{
              detailItem.kind === 'image'
                ? '剪贴板图片 · ' + formatTime(detailItem.copiedAt)
                : '剪贴板内容 · ' + formatTime(detailItem.copiedAt)
            }}
          </div>
          <img
            v-if="detailItem.kind === 'image'"
            :src="thumbFor(detailItem)"
            class="detail-img"
            alt="剪贴板图片"
          />
          <pre v-else class="detail-body">{{ detailItem.content }}</pre>
          <div class="detail-foot">
            <span class="faint"><kbd>←</kbd> 或 <kbd>Esc</kbd> 返回</span>
            <span class="grow" />
            <button class="detail-copy" @click="copy(detailItem)"><Copy :size="13" /> 复制</button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.cb {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.cb-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  height: 52px;
  border-bottom: 1px solid var(--border);
  flex: none;
}

.cb-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: var(--fs-lg);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.cb-count {
  font-size: 11px;
  color: var(--faint);
  background: var(--panel-2);
  border: 1px solid var(--border);
  padding: 1px 7px;
  border-radius: 999px;
}

.cap {
  gap: 8px;
  font-size: 12px;
  flex: none;
}

.search-box {
  position: relative;
  display: flex;
  align-items: center;
}

.search-ico {
  position: absolute;
  left: 9px;
  color: var(--faint);
  pointer-events: none;
}

.search-box input {
  padding: 6px 52px 6px 28px;
  font-size: 12.5px;
  width: 190px;
}

.search-hint {
  position: absolute;
  right: 7px;
  font-size: 9.5px;
  padding: 2px 5px;
  pointer-events: none;
  color: var(--faint);
  background: var(--panel-2);
  transition: opacity var(--t-fast);
}

.search-box:focus-within .search-hint {
  opacity: 0;
}

/* 搜索框与清空之间的发丝分隔线 */
.head-sep {
  width: 1px;
  height: 18px;
  background: var(--border-strong);
  flex: none;
}

.list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 6px 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.list.fade-top {
  mask-image: linear-gradient(to bottom, transparent 0, #000 14px);
}

.list.fade-bottom {
  mask-image: linear-gradient(to bottom, #000 calc(100% - 14px), transparent 100%);
}

.list.fade-top.fade-bottom {
  mask-image: linear-gradient(
    to bottom,
    transparent 0,
    #000 14px,
    #000 calc(100% - 14px),
    transparent 100%
  );
}

/* 搜索命中行（与 .truncated 一族同声部：小号、弱色） */
.hitline {
  font-size: 11px;
  color: var(--faint);
  padding: 2px 4px 2px;
}

/* 时间分组头 */
.grp {
  font-size: 11px;
  font-weight: 600;
  color: var(--faint);
  letter-spacing: 0.02em;
  padding: 10px 4px 0;
}

.truncated {
  text-align: center;
  font-size: 11.5px;
  padding: 6px 0;
}

/* 扁平行 */
.row-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border-radius: var(--r-sm);
  transition: background var(--t-fast);
  cursor: pointer;
}

.row-item:hover {
  background: var(--panel-2);
}

/* 类型图标独立列 */
.kind-ico {
  flex: none;
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  border-radius: var(--r-xs);
  color: var(--faint);
  background: var(--panel-2);
  border: 1px solid var(--border);
}

.content {
  flex: 1;
  min-width: 0;
  user-select: text;
  color: var(--text-2);
  font-size: 12.5px;
  line-height: 1.55;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: break-all;
}

.clip-img {
  height: 52px;
  max-width: 180px;
  object-fit: cover;
  border: 1px solid var(--border);
  border-radius: var(--r-xs);
  background: var(--panel-2);
  flex: none;
  /* 图片行：缩略图靠左，时间与操作推到行尾 */
  margin-right: auto;
}

.row-side {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}

.time {
  font-size: 10.5px;
  color: var(--faint);
}

.ops {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity var(--t-fast);
}

.row-item:hover .ops,
.row-item:focus-within .ops {
  opacity: 1;
}

.ghost-btn {
  background: transparent;
  border-color: transparent;
  color: var(--muted);
  font-size: 12px;
  padding: 4px 8px;
}

.ghost-btn:hover {
  color: var(--text);
  background: var(--panel-3);
  border-color: var(--border);
}

.ghost-btn.danger:hover {
  color: var(--danger);
}

/* 操作钮保持可点的最小命中面积 */
.ops .ghost-btn.sm {
  min-width: 24px;
  min-height: 24px;
  padding: 0 4px;
}

/* 全文浮层 */
.detail-mask {
  position: fixed;
  inset: 0;
  background: var(--mask-bg);
  display: grid;
  place-items: center;
  z-index: var(--z-overlay);
  padding: 22px;
}

.detail {
  width: min(640px, 100%);
  max-height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-2);
  padding: 14px 16px;
  gap: 10px;
}

.detail-title {
  font-weight: 650;
  font-size: var(--fs-lg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-body {
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-y: auto;
  color: var(--text-2);
  max-width: 65ch;
  user-select: text;
}

.detail-img {
  align-self: flex-start;
  max-width: 100%;
  max-height: 420px;
  object-fit: contain;
  border: 1px solid var(--border);
  border-radius: var(--r-xs);
  background: var(--panel-2);
}

.detail-foot {
  display: flex;
  align-items: center;
  font-size: 12px;
}

.detail-foot kbd {
  font-size: 9.5px;
}

.detail-copy {
  padding: 5px 12px;
  font-size: 12px;
}

.detail-enter-active,
.detail-leave-active {
  transition: opacity 180ms var(--ease);
}

.detail-enter-from,
.detail-leave-to {
  opacity: 0;
}
</style>
