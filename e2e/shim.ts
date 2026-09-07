import type { Page } from "@playwright/test";

/**
 * 浏览器内 Tauri IPC shim：
 * 页面加载前注入 window.__TAURI_INTERNALS__，后接一个内存版 fake-backend
 * 实现 api.ts 用到的全部命令。唯一被 mock 的对象是无法进入浏览器的 Rust 后端；
 * 命令名与参数形状由 src/lib/api.contract.test.ts 的契约测试看住。
 * 状态存 localStorage，页面 reload 后仍在，用于验证「重启后数据保留」。
 */

export interface ShimSeed {
  label: string;
  prompts: Array<Record<string, unknown>>;
  categories: string[];
  /** 入库样例导入文件 e2e/fixtures/sample-import.json 的内容，供导入流程使用 */
  importSample: string;
  /** 应用内更新状态（可选）：注入 { kind: "available", version, notes } 驱动更新区用例，缺省视为已是最新 */
  updateStatus?: Record<string, unknown>;
  /** true 时丢弃 localStorage 里的历史状态，从种子重新开始 */
  fresh?: boolean;
}

/** 注入到页面的调试句柄，供 Playwright 用例断言与状态重置 */
declare global {
  interface Window {
    __PM_FAKE__: {
      state: Record<string, any>;
      reset: () => void;
      pastes: () => Array<{ text: string; promptId: string | null }>;
      calls: () => Record<string, number>;
    };
  }
}

export const tauriShim = (seed: ShimSeed) => {
  const w = window as unknown as Record<string, any>;
  const STORE_KEY = "pm-fake-state";
  // fresh 只在该标签页的首次加载时生效：sessionStorage 跨 reload 保留，
  // 这样用例内 reload 模拟「重启」时状态仍在，新用例（新 context）则从种子开始
  const INIT_MARKER = "pm-fake-initialized";
  if (seed.fresh && !sessionStorage.getItem(INIT_MARKER)) {
    try {
      localStorage.removeItem(STORE_KEY);
      sessionStorage.setItem(INIT_MARKER, "1");
    } catch {
      /* 忽略 */
    }
  }

  const defaults = () => ({
    seq: 100,
    data: {
      version: 1,
      seeded: true,
      settings: {
        hotkey: "alt+q",
        captureHotkey: "alt+s",
        captureClipboard: true,
        restoreClipboard: true,
        pasteAppendEnter: false,
        theme: "light",
        webdav: { enabled: false, autoSync: false, url: "", username: "", password: "" },
        gist: { enabled: false, autoSync: false, token: "", gistId: "" },
        syncProvider: "webdav",
        syncClipboard: false,
      },
      categories: [...seed.categories],
      prompts: [...seed.prompts],
      clipboard: [] as any[],
      tombstones: [],
    },
    varMemory: {} as Record<string, Record<string, string>>,
    pastes: [] as Array<{ text: string; promptId: string | null }>,
    calls: {} as Record<string, number>,
    lastExport: null as null | { kind: string; includeClipboard: boolean; path: string },
    lastClipboard: null as string | null,
    autostart: false,
    recoveryNotice: null as string | null,
    updateStatus: JSON.parse(
      JSON.stringify(seed.updateStatus ?? { kind: "upToDate" }),
    ) as Record<string, unknown>,
  });

  const state = (() => {
    try {
      const raw = localStorage.getItem(STORE_KEY);
      if (raw) return JSON.parse(raw);
    } catch {
      /* 忽略 */
    }
    return defaults();
  })();

  const persist = () => localStorage.setItem(STORE_KEY, JSON.stringify(state));
  const now = () => Date.now();

  w.__PM_FAKE__ = {
    state,
    /** 把状态重置回初始种子并持久化（测试隔离用） */
    reset: () => {
      Object.assign(state, defaults());
      persist();
    },
    pastes: () => state.pastes,
    calls: () => state.calls,
  };

  const backend: Record<string, (args: any) => any> = {
    get_data: () => JSON.parse(JSON.stringify(state.data)),
    get_recovery_notice: () => {
      const n = state.recoveryNotice;
      state.recoveryNotice = null;
      persist();
      return n;
    },
    save_prompt: ({ prompt }) => {
      const p = { ...prompt };
      if (!p.id) p.id = `fake${++state.seq}`;
      if (!p.createdAt) p.createdAt = now();
      p.updatedAt = now();
      const i = state.data.prompts.findIndex((x: any) => x.id === p.id);
      if (i >= 0) state.data.prompts[i] = p;
      else state.data.prompts.push(p);
      persist();
      return null;
    },
    delete_prompt: ({ id }) => {
      state.data.prompts = state.data.prompts.filter((x: any) => x.id !== id);
      persist();
      return null;
    },
    record_prompt_use: ({ id }) => {
      const p = state.data.prompts.find((x: any) => x.id === id);
      if (p) {
        p.useCount = (p.useCount ?? 0) + 1;
        p.lastUsedAt = now();
      }
      persist();
      return null;
    },
    add_category: ({ name }) => {
      if (name && !state.data.categories.includes(name)) state.data.categories.push(name);
      persist();
      return null;
    },
    rename_category: ({ oldName, newName }) => {
      // 与真实后端一致：空新名整体验收拒绝，不做部分迁移
      if (!newName) return null;
      const i = state.data.categories.indexOf(oldName);
      if (i >= 0) state.data.categories[i] = newName;
      for (const p of state.data.prompts) if (p.category === oldName) p.category = newName;
      persist();
      return null;
    },
    delete_category: ({ name }) => {
      state.data.categories = state.data.categories.filter((c: string) => c !== name);
      for (const p of state.data.prompts) if (p.category === name) p.category = "未分类";
      persist();
      return null;
    },
    copy_text: ({ text }) => {
      state.lastClipboard = text;
      state.data.clipboard.unshift({ id: `c${++state.seq}`, content: text, copiedAt: now(), kind: "text" });
      persist();
      return null;
    },
    invoke_paste: ({ text, promptId }) => {
      state.pastes.push({ text, promptId: promptId ?? null });
      state.calls.invoke_paste = (state.calls.invoke_paste ?? 0) + 1;
      persist();
      return null;
    },
    delete_history_item: ({ id }) => {
      state.data.clipboard = state.data.clipboard.filter((c: any) => c.id !== id);
      persist();
      return null;
    },
    clear_history: () => {
      state.data.clipboard = [];
      persist();
      return null;
    },
    paste_text_direct: ({ text }) => {
      state.pastes.push({ text, promptId: null });
      persist();
      return null;
    },
    get_image_thumb: () => "",
    paste_image: () => null,
    copy_image: () => null,
    get_clipboard_text: () => state.lastClipboard,
    get_var_memory: ({ promptId }) => state.varMemory[promptId] ?? {},
    save_var_memory: ({ promptId, values }) => {
      state.varMemory[promptId] = values;
      persist();
      return null;
    },
    hide_quick: () => {
      state.calls.hide_quick = (state.calls.hide_quick ?? 0) + 1;
      return null;
    },
    open_manager: () => {
      state.calls.open_manager = (state.calls.open_manager ?? 0) + 1;
      return null;
    },
    close_capture: () => null,
    set_panel_height: () => null,
    save_settings: ({ settings }) => {
      state.data.settings = settings;
      persist();
      // 与真实后端一致：设置保存后派发 data-changed（主题等即时生效依赖它）
      emitEvent("data-changed");
      return null;
    },
    get_autostart: () => state.autostart,
    set_autostart: ({ enable }) => {
      state.autostart = enable;
      persist();
      return null;
    },
    open_data_dir: () => {
      state.calls.open_data_dir = (state.calls.open_data_dir ?? 0) + 1;
      return null;
    },
    // 应用内更新：检查只读状态；安装先派发进度事件再把状态归为最新（真实后端安装后会重启进程）
    check_for_update: () => JSON.parse(JSON.stringify(state.updateStatus)),
    download_and_install_update: () => {
      if (state.updateStatus?.kind !== "available") throw "no update available";
      emitEvent("update://progress", { downloaded: 100, total: 100, percent: 100 });
      state.updateStatus = { kind: "upToDate" };
      persist();
      return null;
    },
    open_release_page: () => null,
    export_data: ({ kind, includeClipboard }) => {
      const path = `C:\\Prompt Tool-Export\\prompt-tool.${kind === "json" ? "json" : "md"}`;
      state.lastExport = { kind, includeClipboard, path };
      persist();
      return path;
    },
    import_data: () => {
      // 真实文件对话框在浏览器里不可用：用随测试注入的仓库真实样例文件代替
      try {
        const parsed = JSON.parse(seed.importSample);
        const list = parsed.prompts ?? [];
        let added = 0;
        let skipped = 0;
        for (const item of list) {
          const dup = state.data.prompts.some(
            (p: any) => p.id === item.id || (p.category === item.category && p.title === item.title),
          );
          if (dup) {
            skipped++;
            continue;
          }
          state.data.prompts.push({
            tags: [],
            pinned: false,
            hotkey: "",
            useCount: 0,
            lastUsedAt: 0,
            ...item,
            createdAt: now(),
            updatedAt: now(),
          });
          if (item.category && !state.data.categories.includes(item.category))
            state.data.categories.push(item.category);
          added++;
        }
        persist();
        return { added, skipped, message: `导入完成：新增 ${added} 条，跳过 ${skipped} 条` };
      } catch {
        return { added: 0, skipped: 0, message: "导入失败" };
      }
    },
    import_paths: () => ({ added: 0, skipped: 0, message: "浏览器环境不支持拖拽导入" }),
    webdav_test: () => {
      throw "尚未配置 WebDAV 地址，请在「云同步」页填写";
    },
    gist_test: () => {
      throw "请先填写 GitHub Token";
    },
    sync_now: () => {
      throw "尚未配置 WebDAV 地址，请在「云同步」页填写";
    },
    // plugin:event|listen / unlisten：真实记录回调，供 emitEvent 派发
    "plugin:event|listen": ({ handler }) => Number(handler),
    "plugin:event|unlisten": ({ eventId }) => {
      callbacks.delete(eventId as number);
      return null;
    },
    // 无框窗口的自定义标题栏：记录窗口控制调用供用例断言（真实窗口行为由全栈 e2e 看住）
    "plugin:window|minimize": () => {
      state.calls.win_minimize = (state.calls.win_minimize ?? 0) + 1;
      return null;
    },
    "plugin:window|toggle_maximize": () => {
      state.winMaximized = !state.winMaximized;
      state.calls.win_toggle_maximize = (state.calls.win_toggle_maximize ?? 0) + 1;
      return null;
    },
    "plugin:window|is_maximized": () => state.winMaximized === true,
    "plugin:window|unmaximize": () => {
      state.winMaximized = false;
      state.calls.win_unmaximize = (state.calls.win_unmaximize ?? 0) + 1;
      return null;
    },
    "plugin:window|start_dragging": () => {
      state.calls.win_start_dragging = (state.calls.win_start_dragging ?? 0) + 1;
      return null;
    },
    "plugin:window|close": () => {
      state.calls.win_close = (state.calls.win_close ?? 0) + 1;
      return null;
    },
  };

  let cbSeq = 1;
  const callbacks = new Map<number, (arg: unknown) => void>();
  /** 模拟后端 emit：把事件派发给所有已注册的 Tauri 事件回调 */
  const emitEvent = (event: string, payload: unknown = null) => {
    for (const [id, cb] of [...callbacks]) cb({ event, id, payload });
  };
  w.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: seed.label },
      currentWebview: { label: seed.label, windowLabel: seed.label },
    },
    transformCallback: (cb: (arg: unknown) => void) => {
      const id = cbSeq++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => {
      callbacks.delete(id);
    },
    convertFileSrc: (p: string) => p,
    invoke: (cmd: string, args: Record<string, unknown> = {}) => {
      const fn = backend[cmd];
      if (!fn) {
        console.warn(`[pm-shim] 未实现的命令: ${cmd}`);
        return Promise.reject(`[pm-shim] 未实现的命令: ${cmd}`);
      }
      try {
        return Promise.resolve(fn(args));
      } catch (e) {
        return Promise.reject(String(e));
      }
    },
  };
};

export async function installShim(page: Page, seed: ShimSeed) {
  // Playwright 无法序列化跨模块导入的函数，这里手动序列化函数源码并内联种子参数
  const source = `(${tauriShim.toString()})(${JSON.stringify(seed)});`;
  await page.addInitScript(source);
}
