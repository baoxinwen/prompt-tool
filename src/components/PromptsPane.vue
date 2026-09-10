<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import {
  Plus,
  Search,
  Copy,
  Trash2,
  Pin,
  PinOff,
  ClipboardPaste,
  Sparkles,
  Keyboard,
  Pencil,
  X,
} from 'lucide-vue-next';
import { api } from '../lib/api';
import { managerKey } from '../lib/context';
import { hasVars, extractVars, isAutoVar, applyVars, applyClipboardVar, clipboardVarRe, VAR_RE } from '../lib/vars';
import { matchText, highlightSegs } from '../lib/search';
import { previewSegments } from '../lib/preview';
import { categoryColor } from '../lib/categoryColor';
import { emptyPrompt } from '../types';
import type { Prompt, VarField } from '../types';
import HotkeyInput from './HotkeyInput.vue';
import EmptyState from './ui/EmptyState.vue';
import CategoryBadge from './ui/CategoryBadge.vue';
import AccentButton from './ui/AccentButton.vue';
import KeyCap from './ui/KeyCap.vue';

const ctx = inject(managerKey)!;

const varMark = '{{变量}}';
/** 自动保存防抖：停止输入 900ms 后静默落盘 */
const AUTO_SAVE_DELAY = 900;
/** 「✓ 已自动保存」提示停留时长 */
const SAVED_TIP_MS = 2400;
/** 复制按钮「已复制 ✓」回弹时长 */
const COPY_TIP_MS = 1200;

const selectedCategory = ref('');
const query = ref('');
const selectedId = ref('');
const draft = ref<Prompt | null>(null);
const snapshot = ref('');
const dirty = ref(false);
const newCatName = ref('');
const addingCat = ref(false);
const editingCat = ref('');
const editName = ref('');
// v-for 内的字符串 ref 会被收集成数组（focus 调用会抛 TypeError），
// 用函数 ref 拿到单个输入框；同一时刻至多一个分类处于重命名态
const renameInput = ref<HTMLInputElement | null>(null);
function setRenameInput(el: unknown) {
  renameInput.value = (el as HTMLInputElement | null) ?? null;
}
const searchInput = ref<HTMLInputElement | null>(null);

/** 自动保存状态机：idle 无指示 / dirty ● 未保存 / saved ✓ 已自动保存 */
const saveState = ref<'idle' | 'dirty' | 'saved'>('idle');
/** 变量卡当前值（不属提示词正文，不参与 dirty/自动保存）。
 *  必须用无原型对象：变量名用户可控（如 __proto__），普通 {} 的赋值会被
 *  原型访问器吞掉导致输入静默丢失（评审 2026-09-10 C2，与 VarDialog 同口径） */
const varValues = ref<Record<string, string>>(Object.create(null) as Record<string, string>);
/** 填写变量并粘贴进行中（防重复点击） */
const pasteBusy = ref(false);
/** 复制按钮「已复制 ✓」短暂态 */
const copied = ref(false);
const contentEl = ref<HTMLTextAreaElement | null>(null);
const mirrorEl = ref<HTMLElement | null>(null);

let autoSaveTimer: ReturnType<typeof setTimeout> | undefined;
let savedTipTimer: ReturnType<typeof setTimeout> | undefined;
let copiedTimer: ReturnType<typeof setTimeout> | undefined;
let varLoadSeq = 0;

const allPrompts = computed(() => ctx.data.value?.prompts ?? []);
const categories = computed(() => ctx.data.value?.categories ?? []);

const shown = computed(() => {
  let list = allPrompts.value;
  if (selectedCategory.value) list = list.filter((p) => p.category === selectedCategory.value);
  const q = query.value.trim().toLowerCase();
  if (q) {
    list = list.filter(
      (p) =>
        matchText(p.title, q) ||
        p.content.toLowerCase().includes(q) ||
        p.tags.some((t) => matchText(t, q)),
    );
  }
  const sorted = [...list].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.updatedAt - a.updatedAt;
  });
  // 大列表截断渲染，避免 DOM 堆积
  return query.value.trim() ? sorted : sorted.slice(0, 200);
});

const catCounts = computed(() => {
  const m = new Map<string, number>();
  for (const p of allPrompts.value) m.set(p.category, (m.get(p.category) ?? 0) + 1);
  return m;
});

const draftColor = computed(() => categoryColor(draft.value?.category ?? ''));

/** 手动变量数（排除 {{clipboard}} 等自动变量，与 QuickPanel/VarDialog 口径一致） */
function manualVarCount(content: string): number {
  return extractVars(content).filter((v) => !isAutoVar(v.name)).length;
}

/** 当前草稿的手动变量卡（含 hint，供 placeholder；{{clipboard}} 自动变量不进卡） */
const draftVars = computed<VarField[]>(() =>
  draft.value ? extractVars(draft.value.content).filter((v) => !isAutoVar(v.name)) : [],
);

/** 变量名转 {{名}} 记号文本（模板里不能直接写字面量闭合花括号） */
function varChip(name: string) {
  return '{{' + name + '}}';
}

async function openPrompt(p: Prompt) {
  await flushAutoSave();
  if (!(await confirmDiscard())) return;
  selectedId.value = p.id;
  loadDraft(p);
  void loadVarValues();
}

function loadDraft(p: Prompt) {
  draft.value = JSON.parse(JSON.stringify(p));
  snapshot.value = JSON.stringify(draft.value);
  dirty.value = false;
  saveState.value = 'idle';
  clearTimeout(savedTipTimer);
}

async function newPrompt() {
  await flushAutoSave();
  if (!(await confirmDiscard())) return;
  selectedId.value = '';
  draft.value = emptyPrompt(selectedCategory.value);
  snapshot.value = '';
  dirty.value = false;
  saveState.value = 'idle';
  varValues.value = Object.create(null) as Record<string, string>;
  clearTimeout(savedTipTimer);
}

async function confirmDiscard(): Promise<boolean> {
  if (!dirty.value) return true;
  return ctx.confirm({
    title: '放弃未保存的修改？',
    message: '当前提示词的修改还没有保存。',
    confirmText: '放弃修改',
    danger: true,
  });
}

function markDirty() {
  if (!draft.value) return;
  dirty.value = JSON.stringify(draft.value) !== snapshot.value;
  if (dirty.value) {
    saveState.value = 'dirty';
    scheduleAutoSave();
  }
}

/**
 * 保存核心：手动路径（toast + 重载草稿）与自动路径（静默，防光标跳动）共用。
 * 静默路径不 toast、不重载草稿：只把后端回写的 id/审计字段原位同步进草稿并
 * 更新快照；保存期间若又有输入，dirty 会被重新算回 true 等下一轮自动保存
 */
async function saveCore(opts: { silent?: boolean } = {}): Promise<boolean> {
  if (!draft.value) return false;
  if (!draft.value.title.trim()) {
    if (!opts.silent) ctx.toast('请填写标题', 'err');
    return false;
  }
  // 本次落盘的状态基线（深拷贝）+ 竞态基线：await 之后 ctx.refresh 可能触发
  // watch(allPrompts) 重载草稿（loadDraft 会替换 draft.value 的引用），
  // 只有同一份草稿对象才允许被续体回写（评审 2026-09-10 I4）
  const saving = JSON.parse(JSON.stringify(draft.value)) as Prompt;
  const draftAtSave = draft.value;
  const wasNew = !selectedId.value;
  let savedId: string;
  try {
    // 后端回传建档 id：不再用「createdAt 最新」启发式反查——并发建档
    // （如全局快速捕获）会让启发式绑错条目，随后的自动保存会覆盖别人的
    // 数据（评审 2026-09-10 I5）
    savedId = await api.savePrompt({ ...draft.value });
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
    return false;
  }
  if (opts.silent) {
    if (draft.value === draftAtSave) {
      const saved = allPrompts.value.find((p) => p.id === savedId);
      if (saved) {
        if (wasNew) {
          // 新建提示词首次自动保存：把后端 id 写回草稿，下一轮改为更新
          selectedId.value = saved.id;
          saving.id = saved.id;
          draft.value.id = saved.id;
        }
        saving.updatedAt = saved.updatedAt;
        saving.useCount = saved.useCount;
        saving.lastUsedAt = saved.lastUsedAt;
        draft.value.updatedAt = saved.updatedAt;
        draft.value.useCount = saved.useCount;
        draft.value.lastUsedAt = saved.lastUsedAt;
      }
      snapshot.value = JSON.stringify(saving);
      dirty.value = JSON.stringify(draft.value) !== snapshot.value;
    }
    // 草稿已被切换/重载：不碰新草稿、不动 snapshot，新草稿的状态由 loadDraft 决定
    return true;
  }
  ctx.toast('已保存');
  if (draft.value === draftAtSave) {
    const saved = allPrompts.value.find((p) => p.id === (savedId || selectedId.value));
    if (saved) {
      selectedId.value = saved.id;
      loadDraft(saved);
    }
  }
  return true;
}

async function save() {
  // 手动保存立即落盘，挂起的自动保存不再需要
  clearTimeout(autoSaveTimer);
  autoSaveTimer = undefined;
  if (await saveCore()) {
    saveState.value = 'idle';
    clearTimeout(savedTipTimer);
  } else if (dirty.value) {
    // 失败分支重排自动保存（评审 2026-09-10 M2#7）：否则自动保存链
    // 在此处断掉，状态灯停在「未保存」直到用户再次键入
    scheduleAutoSave();
  }
}

// ---------- 自动保存（防抖静默落盘） ----------

function scheduleAutoSave() {
  clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(() => {
    autoSaveTimer = undefined;
    void autoSave();
  }, AUTO_SAVE_DELAY);
}

async function autoSave() {
  if (!draft.value || !dirty.value) return;
  const ok = await saveCore({ silent: true });
  if (!draft.value) return; // 保存期间草稿被关闭/删除
  if (ok && !dirty.value) {
    saveState.value = 'saved';
    clearTimeout(savedTipTimer);
    savedTipTimer = setTimeout(() => {
      if (saveState.value === 'saved') saveState.value = 'idle';
    }, SAVED_TIP_MS);
  } else {
    // 失败（或保存期间又有输入）：保持「● 未保存」
    saveState.value = 'dirty';
  }
}

/** 立即触发挂起的自动保存：切换提示词/新建前调用，能存则存，避免误弹「放弃修改」 */
async function flushAutoSave() {
  if (!autoSaveTimer) return;
  clearTimeout(autoSaveTimer);
  autoSaveTimer = undefined;
  await autoSave();
}

async function remove() {
  if (!draft.value?.id) return;
  const removed = draft.value;
  const ok = await ctx.confirm({
    title: `删除「${removed.title}」？`,
    message: '删除后可在通知条中撤销。',
    confirmText: '删除',
    danger: true,
  });
  if (!ok) return;
  try {
    await api.deletePrompt(removed.id);
    selectedId.value = '';
    draft.value = null;
    await ctx.refresh();
    ctx.toast(`已删除「${removed.title}」`, 'ok', {
      label: '撤销',
      handler: async () => {
        try {
          await api.savePrompt(removed);
          await ctx.refresh();
          ctx.toast('已恢复');
        } catch (e) {
          ctx.toast(String(e), 'err');
        }
      },
    });
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

async function togglePin() {
  if (!draft.value) return;
  draft.value.pinned = !draft.value.pinned;
  markDirty();
}

function onTagInput(e: Event) {
  if (!draft.value) return;
  const v = (e.target as HTMLInputElement).value;
  draft.value.tags = v
    .split(/[,，]/)
    .map((s) => s.trim())
    .filter(Boolean);
  markDirty();
}

/** 复制正文原文（{{变量}} 占位符不替换），按钮短暂显示「已复制 ✓」 */
async function copyContent() {
  if (!draft.value?.content) return;
  try {
    await api.copyText(draft.value.content);
    copied.value = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied.value = false), COPY_TIP_MS);
    ctx.toast('已复制，含 {{变量}} 占位符原文');
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

// ---------- 变量卡：记忆预填 + 填写并粘贴 ----------

/** 打开提示词时重置变量卡并按变量记忆预填（openPrompt 显式调用）。
 *  保存后的草稿重载/自动建档不经过这里，已输入的变量值不会被清掉 */
async function loadVarValues() {
  const id = draft.value?.id ?? '';
  const seq = ++varLoadSeq;
  const values: Record<string, string> = Object.create(null);
  for (const f of draftVars.value) values[f.name] = '';
  if (id) {
    try {
      const mem = await api.getVarMemory(id);
      if (seq !== varLoadSeq || draft.value?.id !== id) return; // 已切到别的提示词
      for (const f of draftVars.value) if (mem[f.name]) values[f.name] = mem[f.name];
    } catch {
      /* 记忆读取失败不影响使用 */
    }
  }
  if (seq !== varLoadSeq) return;
  varValues.value = values;
}

/** {{clipboard}} 自动变量：粘贴前用当前剪贴板文本填充
 *  （QuickPanel doPaste 同款语义；读取失败 ≠ 剪贴板为空，文案须区分。
 *  替换必须走 applyClipboardVar 的函数替换：字符串替换会展开剪贴板里的
 *  $&/$$ 序列改写内容——评审 2026-09-10 C1，本地副本已删，统一用 vars.ts） */
async function fillClipboardVar(text: string): Promise<string> {
  if (!clipboardVarRe().test(text)) return text;
  let clip: string | null;
  try {
    clip = await api.getClipboardText();
  } catch (e) {
    ctx.toast(`读取剪贴板失败（${e}），{{clipboard}} 已留空`, 'err');
    return applyClipboardVar(text, null);
  }
  if (!clip) ctx.toast('剪贴板为空，{{clipboard}} 已留空', 'err');
  return applyClipboardVar(text, clip);
}

/** 主操作：变量卡当前值替换占位符 → {{clipboard}} 自动填充 → 粘贴到原活动窗口 */
async function fillAndPaste() {
  if (!draft.value?.content || pasteBusy.value) return;
  pasteBusy.value = true;
  try {
    const fields = draftVars.value;
    let text = applyVars(draft.value.content, varValues.value);
    text = await fillClipboardVar(text);
    const promptId = draft.value.id || undefined;
    await api.invokePaste(text, promptId);
    // 变量值记忆与 VarDialog 同口径回存（失败不阻断粘贴，仅留日志）
    if (promptId && fields.length) {
      const mem: Record<string, string> = Object.create(null);
      for (const f of fields) mem[f.name] = varValues.value[f.name] ?? '';
      api.saveVarMemory(promptId, mem).catch((e) => {
        console.error('[prompt-tool] 变量记忆保存失败:', e);
      });
    }
    const filled = fields.filter((f) => varValues.value[f.name]).length;
    ctx.toast(fields.length ? `已填写 ${filled}/${fields.length} 个变量并粘贴` : '已粘贴');
  } catch (e) {
    ctx.toast(String(e), 'err');
  } finally {
    pasteBusy.value = false;
  }
}

// ---------- 镜像高亮编辑器 ----------

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** 镜像层高亮 HTML：先整体 HTML 转义，再把 {{…}}（VAR_RE 全片段）包上
 *  琥珀记号 span。span 只变色不加盒模型，保证与 textarea 逐字符对齐 */
function hlHTML(text: string): string {
  let out = '';
  let last = 0;
  VAR_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = VAR_RE.exec(text)) !== null) {
    out += escHtml(text.slice(last, m.index));
    out += `<span class="var-mark">${escHtml(m[0])}</span>`;
    last = m.index + m[0].length;
  }
  out += escHtml(text.slice(last));
  return out;
}

/** 末尾补一个换行：pre 的收尾空行与 textarea 滚动高度保持一致 */
const mirrorHtml = computed(() => hlHTML(draft.value?.content ?? '') + '\n');

/** textarea → 镜像层滚动同步（镜像层自身不可滚动、不接收指针） */
function syncMirrorScroll() {
  const ta = contentEl.value;
  const mir = mirrorEl.value;
  if (!ta || !mir) return;
  mir.scrollTop = ta.scrollTop;
  mir.scrollLeft = ta.scrollLeft;
}

watch(
  () => draft.value?.content,
  () => nextTick(syncMirrorScroll),
);

// ---------- 分类管理 ----------

async function addCategory() {
  const name = newCatName.value.trim();
  if (!name) return;
  try {
    await api.addCategory(name);
    newCatName.value = '';
    addingCat.value = false;
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

function startRename(name: string) {
  editingCat.value = name;
  editName.value = name;
  nextTick(() => renameInput.value?.focus());
}
async function saveRename(oldName: string) {
  if (editingCat.value !== oldName) return;
  editingCat.value = '';
  const name = editName.value.trim();
  if (!name || name === oldName) return;
  try {
    await api.renameCategory(oldName, name);
    if (selectedCategory.value === oldName) selectedCategory.value = name;
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

/** 输入法组合态的 Enter/Esc 是候选词上屏/取消，不得提交/关闭编辑器（评审 I2） */
function isComposingEvent(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}

function onRenameKeydown(e: KeyboardEvent, oldName: string) {
  if (isComposingEvent(e)) return;
  if (e.key === 'Enter') saveRename(oldName);
  else if (e.key === 'Escape') editingCat.value = '';
}

function onAddCategoryKeydown(e: KeyboardEvent) {
  if (isComposingEvent(e)) return;
  if (e.key === 'Enter') addCategory();
  else if (e.key === 'Escape') addingCat.value = false;
}

// 新建分类输入框出现即聚焦：与重命名流程（nextTick focus）同口径，
// 否则要点两次才能输入（评审 2026-09-10 M2#5）
const addCatInput = ref<HTMLInputElement | null>(null);
function toggleAddingCat() {
  addingCat.value = !addingCat.value;
  if (addingCat.value) nextTick(() => addCatInput.value?.focus());
}

async function deleteCategory(name: string) {
  const ok = await ctx.confirm({
    title: `删除分类「${name}」？`,
    message: '其中的提示词将移入「未分类」。',
    confirmText: '删除',
    danger: true,
  });
  if (!ok) return;
  try {
    await api.deleteCategory(name);
    if (selectedCategory.value === name) selectedCategory.value = '';
    await ctx.refresh();
  } catch (e) {
    ctx.toast(String(e), 'err');
  }
}

// ---------- 快捷键 Ctrl+S 保存 / Ctrl+K 聚焦搜索 ----------

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    save();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    searchInput.value?.focus();
  }
}

onMounted(() => {
  // 离开守卫：有未保存草稿时 Manager 切换标签前会先弹确认，
  // 防止 v-if 卸载把草稿静默丢掉（评审 I6）
  ctx.setLeaveGuard(() => !dirty.value);
  document.addEventListener('keydown', onKeydown);
  window.addEventListener('pm-focus-search', focusSearch);
});
onBeforeUnmount(() => {
  ctx.setLeaveGuard(null);
  document.removeEventListener('keydown', onKeydown);
  window.removeEventListener('pm-focus-search', focusSearch);
  clearTimeout(autoSaveTimer);
  clearTimeout(savedTipTimer);
  clearTimeout(copiedTimer);
});

function focusSearch() {
  searchInput.value?.focus();
}

// 列表变化时保持选中项的草稿同步（例如云同步覆盖）。
// 列表项与快照一致（多为刚保存后的数据回写）时不再重载草稿，
// 避免替换草稿对象造成输入光标/变量卡状态无谓抖动
watch(allPrompts, () => {
  if (selectedId.value && !dirty.value) {
    const p = allPrompts.value.find((x) => x.id === selectedId.value);
    if (p && JSON.stringify(p) !== snapshot.value) loadDraft(p);
  }
});

function fmtTime(ts: number) {
  return ts ? new Date(ts).toLocaleString() : '—';
}
</script>

<template>
  <div class="pp">
    <!-- 上下文工具栏：页名 + 计数 + 搜索 + 新建 -->
    <header class="pp-head">
      <h2 class="pp-title">
        提示词
        <span class="pp-count tnum">{{ allPrompts.length }}</span>
      </h2>
      <span class="grow" />
      <div class="search-box">
        <Search :size="14" class="search-ico" />
        <input
          ref="searchInput"
          v-model="query"
          type="text"
          placeholder="搜索标题 / 标签 / 内容 / 拼音…"
          spellcheck="false"
        />
        <kbd class="search-hint">Ctrl K</kbd>
      </div>
      <AccentButton @click="newPrompt"><Plus :size="15" style="margin-right: 5px" />新建</AccentButton>
    </header>

    <!-- 双栏：列表 + 详情 -->
    <div class="pp-body">
      <div class="col-list">
        <!-- 分类 chips：只占列表宽度，归还编辑器纵向空间 -->
        <div class="cat-row">
          <button class="chip" :class="{ on: selectedCategory === '' }" @click="selectedCategory = ''">
            全部 <span class="chip-n tnum">{{ allPrompts.length }}</span>
          </button>
          <template v-for="c in categories" :key="c">
            <input
              v-if="editingCat === c"
              :ref="setRenameInput"
              v-model="editName"
              class="chip-input"
              placeholder="新名称，回车确认"
              @keydown="onRenameKeydown($event, c)"
              @blur="saveRename(c)"
            />
            <button
              v-else
              class="chip"
              :class="{ on: selectedCategory === c }"
              @click="selectedCategory = selectedCategory === c ? '' : c"
            >
              <span class="chip-dot" :style="{ background: categoryColor(c).main }" />
              {{ c }}
              <span class="chip-n tnum">{{ catCounts.get(c) ?? 0 }}</span>
              <span class="chip-ops" @click.stop>
                <Pencil :size="11" title="重命名" @click="startRename(c)" />
                <X :size="11" title="删除" @click="deleteCategory(c)" />
              </span>
            </button>
          </template>
          <button class="chip chip-add" title="新建分类" @click="toggleAddingCat">
            <Plus :size="12" />
          </button>
          <input
            v-if="addingCat"
            ref="addCatInput"
            v-model="newCatName"
            class="chip-input"
            placeholder="分类名，回车确认"
            @keydown="onAddCategoryKeydown"
          />
        </div>

        <!-- 列表：扁平行 -->
        <div class="list-scroll">
          <div
            v-for="p in shown"
            :key="p.id"
            class="pitem"
            :class="{ on: p.id === selectedId }"
            @click="openPrompt(p)"
          >
            <div class="pitem-title">
              <Pin v-if="p.pinned" :size="12" class="pin" />
              <template v-for="(seg, si) in highlightSegs(p.title, query)" :key="si">
                <b v-if="seg.hit" class="hl">{{ seg.t }}</b>
                <template v-else>{{ seg.t }}</template>
              </template>
              <span class="grow" />
              <CategoryBadge v-if="p.category" :name="p.category" mode="badge" />
            </div>
            <!-- meta 行：无快捷键/次数/变量时整行不渲染，行高更紧凑 -->
            <div v-if="p.hotkey || p.useCount || manualVarCount(p.content)" class="pitem-row">
              <KeyCap v-if="p.hotkey" :combo="p.hotkey" />
              <span v-if="p.useCount" class="use-n tnum">{{ p.useCount }} 次</span>
              <span v-if="manualVarCount(p.content)" class="var-n tnum">{{ manualVarCount(p.content) }} 个变量</span>
            </div>
            <div class="pitem-preview">
              <template v-for="(seg, si) in previewSegments(p.content, 44)" :key="si">
                <span v-if="seg.chip" class="var-mark">{{ seg.t }}</span>
                <template v-else>{{ seg.t }}</template>
              </template>
            </div>
          </div>
          <EmptyState v-if="!shown.length" :icon="Sparkles" title="暂无提示词">
            点右上角「新建」添加第一条
          </EmptyState>
        </div>
      </div>

      <div class="col-detail">
        <template v-if="draft">
          <!-- 编辑器头：分类色竖条 + 大标题 + 等宽元数据 -->
          <div class="d-hero">
            <span class="d-hero-bar" :style="{ background: draftColor.main }" />
            <div class="d-hero-main">
              <div class="d-hero-top">
                <span class="d-cat">{{ draft.category || '未分类' }}</span>
                <span class="grow" />
                <button class="mini-btn" :class="{ on: draft.pinned }" @click="togglePin">
                  <Pin v-if="draft.pinned" :size="13" />
                  <PinOff v-else :size="13" />
                  {{ draft.pinned ? '已置顶' : '置顶' }}
                </button>
              </div>
              <input v-model="draft.title" class="d-title" placeholder="提示词标题" @input="markDirty" />
              <div class="d-meta tnum">
                使用 {{ draft.useCount }} 次 · 最近 {{ fmtTime(draft.lastUsedAt) }}
                <template v-if="hasVars(draft.content)"> · <span class="mono">{{ varMark }}</span> 待填写</template>
              </div>
            </div>
          </div>

          <div class="d-body">
            <div class="d-card d-card-content">
              <div class="d-card-head">
                <span>内容</span>
                <span class="faint tnum">{{ draft.content.length }} 字符</span>
              </div>
              <!-- 镜像高亮编辑器：pre 与 textarea 同度量，{{…}} 琥珀高亮在镜像层 -->
              <div class="ed-wrap">
                <pre ref="mirrorEl" class="ed-mirror" aria-hidden="true" v-html="mirrorHtml" />
                <textarea
                  ref="contentEl"
                  v-model="draft.content"
                  class="ed-input d-content"
                  spellcheck="false"
                  placeholder="支持 {{变量|说明}} 占位符；{{clipboard}} 自动填入剪贴板内容"
                  @input="markDirty"
                  @scroll="syncMirrorScroll"
                />
              </div>
            </div>

            <!-- 变量卡：真输入框，按变量记忆预填，值只用于粘贴不进正文 -->
            <div v-if="draftVars.length" class="d-card d-card-vars">
              <div class="d-card-head">
                <span>变量</span>
                <span class="faint tnum">{{ draftVars.length }} 个</span>
              </div>
              <div class="var-fields">
                <label v-for="f in draftVars" :key="f.name" class="var-field">
                  <span class="var-mark mono var-name">{{ varChip(f.name) }}</span>
                  <input
                    v-model="varValues[f.name]"
                    class="d-input"
                    :placeholder="f.hint || `填入 ${f.name}`"
                    spellcheck="false"
                  />
                </label>
              </div>
            </div>

            <div class="d-grid2">
              <div class="d-card">
                <div class="d-card-head"><span>分类与标签</span></div>
                <select v-model="draft.category" class="d-input" @change="markDirty">
                  <option v-for="c in categories" :key="c" :value="c">{{ c }}</option>
                  <option v-if="!categories.includes(draft.category)" :value="draft.category">
                    {{ draft.category || '未分类' }}
                  </option>
                </select>
                <input
                  class="d-input"
                  :value="draft.tags.join(', ')"
                  placeholder="标签，逗号分隔"
                  @input="onTagInput"
                />
              </div>
              <div class="d-card">
                <div class="d-card-head">
                  <Keyboard :size="13" style="color: var(--muted)" />
                  <span>全局快捷键</span>
                </div>
                <HotkeyInput
                  :model-value="draft.hotkey"
                  placeholder="如 Ctrl+Alt+1；保存后生效"
                  @update:model-value="
                    (v) => {
                      if (!draft) return;
                      draft.hotkey = v;
                      markDirty();
                    }
                  "
                />
                <div class="faint d-tip">按下快捷键即粘贴此提示词</div>
              </div>
            </div>
          </div>

          <div class="d-foot">
            <button class="ghost-btn danger" :disabled="!draft.id" @click="remove">
              <Trash2 :size="14" /> 删除
            </button>
            <span class="grow" />
            <span
              v-if="saveState !== 'idle'"
              class="data-save"
              :class="saveState"
              aria-live="polite"
            >
              {{ saveState === 'dirty' ? '● 未保存' : '✓ 已自动保存' }}
            </span>
            <!-- 次级 strong（面板底+描边+粗体）：主操作让位给「复制」的实心墨块 -->
            <button
              class="strong-btn fill-paste-btn"
              :disabled="pasteBusy || !draft.content"
              @click="fillAndPaste"
            >
              <ClipboardPaste :size="14" />填写变量并粘贴
            </button>
            <AccentButton class="copy-btn" @click="copyContent">
              <Copy :size="14" />{{ copied ? '已复制 ✓' : '复制' }}
            </AccentButton>
          </div>
        </template>

        <EmptyState v-else :icon="Sparkles" title="从左侧选择提示词">
          或点击右上角「新建」创建一条
          <template #action>
            <AccentButton @click="newPrompt"><Plus :size="14" style="margin-right: 5px" />新建提示词</AccentButton>
          </template>
        </EmptyState>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pp {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* 上下文工具栏 */
.pp-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 16px;
  height: 52px;
  flex: none;
  border-bottom: 1px solid var(--border);
}

.pp-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: var(--fs-lg);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.pp-count {
  font-size: 11px;
  color: var(--faint);
  background: var(--panel-2);
  border: 1px solid var(--border);
  padding: 1px 7px;
  border-radius: 999px;
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
  width: 230px;
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

/* 双栏 */
.pp-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.col-list {
  width: 320px;
  flex: none;
  border-right: 1px solid var(--border);
  background: var(--bg-soft);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* 分类 chips 行 */
.cat-row {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  flex: none;
}

.chip {
  padding: 3.5px 10px;
  border-radius: 999px;
  font-size: 11.5px;
  color: var(--muted);
  background: var(--panel);
  gap: 6px;
}

.chip:hover {
  background: var(--panel-2);
  border-color: var(--border-strong);
}

.chip-n {
  font-size: 10px;
  color: var(--faint);
}

.chip.on {
  background: var(--brand-soft-2);
  border-color: var(--brand);
  color: var(--text);
}

.chip.on .chip-n {
  color: var(--brand);
}

.chip-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  display: inline-block;
}

.chip-ops {
  display: none;
  gap: 4px;
  color: var(--faint);
}

.chip:hover .chip-ops {
  display: inline-flex;
}

.chip-ops svg:hover {
  color: var(--text);
}

.chip-add {
  padding: 3.5px 8px;
}

.chip-input {
  width: 132px;
  padding: 4px 10px;
  font-size: 11.5px;
}

/* 列表：扁平行 */
.list-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px 8px 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.pitem {
  position: relative;
  padding: 9px 11px;
  border-radius: var(--r-sm);
  cursor: pointer;
  transition: background var(--t-fast);
}

.pitem:hover {
  background: var(--panel-2);
}

.pitem.on {
  background: var(--panel);
  box-shadow: inset 0 0 0 1px var(--border-strong);
}

.pitem.on::before {
  content: '';
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 2.5px;
  border-radius: 3px;
  background: var(--brand);
}

.pitem-title {
  display: flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 徽章是 flex 项，禁收缩避免长标题把徽章压扁 */
.pitem-title .badge {
  flex: none;
}

.pin {
  color: var(--warn);
  flex: none;
}

.hl {
  color: var(--brand);
  font-weight: 700;
}

.pitem-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}

.use-n {
  font-size: 10.5px;
  color: var(--faint);
}

/* 「N 个变量」计数 chip：琥珀记号色（--var），与预览 var-mark 同族 */
.var-n {
  font-size: 10px;
  line-height: 1.6;
  color: var(--var);
  background: color-mix(in srgb, var(--var) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--var) 26%, transparent);
  padding: 0 6px;
  border-radius: 999px;
}

.pitem-preview {
  color: var(--faint);
  font-size: 11.5px;
  margin-top: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* {{变量}} chip：琥珀记号色（--var），与 QuickPanel .var-chip 同族 */
.var-mark {
  display: inline-block;
  max-width: 100%;
  padding: 0 5px;
  margin: 0 1px;
  border-radius: var(--r-xs);
  background: color-mix(in srgb, var(--var) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--var) 28%, transparent);
  color: var(--var);
  font-size: 11px;
  line-height: 1.5;
}

/* 详情 */
.col-detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 16px 18px 14px;
}

/* 编辑器头：分类色竖条 */
.d-hero {
  display: flex;
  gap: 14px;
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  background: var(--panel);
  padding: 14px 16px;
  flex: none;
  overflow: hidden;
}

.d-hero-bar {
  width: 3px;
  border-radius: 3px;
  flex: none;
  align-self: stretch;
}

.d-hero-main {
  flex: 1;
  min-width: 0;
}

.d-hero-top {
  display: flex;
  align-items: center;
  gap: 8px;
}

.d-cat {
  font-size: 11.5px;
  font-weight: 650;
  letter-spacing: 0.5px;
  color: var(--muted);
}

.mini-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  padding: 3.5px 10px;
  border-radius: 999px;
  color: var(--muted);
}

.mini-btn.on {
  color: var(--warn);
  border-color: var(--warn);
  background: var(--warn-soft);
}

.d-title {
  border: none;
  background: transparent;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
  padding: 4px 0 2px;
  color: var(--text);
}

.d-title:focus {
  box-shadow: none;
  background: transparent;
}

.d-meta {
  font-size: 11px;
  color: var(--faint);
  margin-top: 2px;
}

.d-meta .mono {
  color: var(--brand);
}

.d-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px 0;
}

.d-card {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.d-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
}

.d-card-content {
  flex: 1;
  min-height: 220px;
}

/* 镜像高亮编辑器：镜像 pre 与输入 textarea 同字体/行高/padding/换行逐字符对齐 */
.ed-wrap {
  position: relative;
  flex: 1;
  min-height: 150px;
}

.ed-mirror,
.ed-input {
  margin: 0;
  width: 100%;
  height: 100%;
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.65;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: break-word;
}

.ed-mirror {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  background: var(--bg-soft);
  color: var(--text-2);
  user-select: none;
}

.ed-input {
  position: relative;
  display: block;
  resize: none;
  /* 文字交给镜像层显示，输入层只留光标与选区 */
  background: transparent;
  color: transparent;
  caret-color: var(--text);
}

.ed-input:focus {
  background: transparent;
}

.ed-input::selection {
  background: color-mix(in srgb, var(--brand) 30%, transparent);
}

/* 镜像层的 {{…}} 记号：只变色不加盒模型，避免破坏与 textarea 的对齐
   （v-html 内容不带 scoped 属性，须用 :deep 穿透） */
.ed-mirror :deep(.var-mark) {
  display: inline;
  padding: 0;
  margin: 0;
  border: none;
  background: none;
  border-radius: 0;
  color: var(--var);
  font-size: inherit;
  line-height: inherit;
}

/* 变量卡：真输入框 */
.d-card-vars {
  flex: none;
}

.var-fields {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 8px 14px;
}

.var-field {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.var-field .d-input {
  flex: 1;
  min-width: 0;
}

.var-name {
  flex: none;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.d-grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  flex: none;
}

.d-input {
  font-size: 12.5px;
  padding: 7px 10px;
}

.d-tip {
  font-size: 11px;
}

.d-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 10px;
  flex: none;
}

/* 自动保存指示：● 未保存 / ✓ 已自动保存 */
.data-save {
  font-size: 11px;
  padding: 3px 10px;
  border-radius: 999px;
  white-space: nowrap;
}

.data-save.dirty {
  color: var(--warn);
  background: var(--warn-soft);
}

.data-save.saved {
  color: var(--ok);
  background: var(--ok-soft);
}

.ghost-btn {
  background: transparent;
  border-color: var(--border);
  color: var(--text-2);
  font-size: 12.5px;
  padding: 7px 14px;
}

/* 删除：红字描边恒定呈现（确认 + 撤销通知条行为不变） */
.ghost-btn.danger {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 32%, transparent);
}

.ghost-btn.danger:hover {
  border-color: var(--danger);
  background: var(--danger-soft);
}

/* 填写变量并粘贴：次级 strong——面板底 + border-strong 描边 + 粗体（非实心墨块） */
.strong-btn {
  font-weight: 600;
  border-color: var(--border-strong);
}
</style>
