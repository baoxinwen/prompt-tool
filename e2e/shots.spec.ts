import { test, type Page } from "@playwright/test";
import { installShim, type ShimSeed } from "./shim";

/** 前后对比截图：SHOT_DIR 环境变量控制输出目录（必须显式指定，默认写到中性目录，
 *  避免跑全量套件时误覆盖珍贵的改前/改后基准） */
const DIR = process.env.SHOT_DIR ?? "e2e-artifacts/shots";

const now = 1_700_000_000_000;
const day = 86_400_000;

interface SeedPrompt {
  id: string;
  title: string;
  content: string;
  category: string;
  tags: string[];
  pinned: boolean;
  hotkey: string;
  useCount: number;
  lastUsedAt: number;
  createdAt: number;
  updatedAt: number;
}

const prompts: SeedPrompt[] = [
  {
    id: "p1", title: "01 需求与方案",
    content: "把我的想法变成可开工的需求与方案文档。项目背景：{{背景|一句话描述项目}}，目标用户：{{用户|谁会用}}",
    category: "开发", tags: ["规划"], pinned: true, hotkey: "",
    useCount: 12, lastUsedAt: now + day * 9, createdAt: now, updatedAt: now + day * 9,
  },
  {
    id: "p2", title: "UI 设计审查",
    content: "请从层级、对比、一致性三个维度审查以下界面方案，给出具体修改意见：\n\n{{方案|粘贴方案说明}}",
    category: "设计", tags: ["审查"], pinned: true, hotkey: "Ctrl+Alt+1",
    useCount: 8, lastUsedAt: now + day * 8, createdAt: now, updatedAt: now + day * 8,
  },
  {
    id: "p3", title: "代码审查",
    content: "请审查以下代码的正确性与可读性，逐条说明理由：\n\n{{代码|贴入代码}}",
    category: "开发", tags: [], pinned: false, hotkey: "",
    useCount: 5, lastUsedAt: now + day * 6, createdAt: now, updatedAt: now + day * 6,
  },
  {
    id: "p4", title: "周报生成",
    content: "本周完成：{{本周工作|逐条列出}}，下周计划：{{下周计划|逐条列出}}。先结论后细节。",
    category: "写作", tags: [], pinned: false, hotkey: "",
    useCount: 3, lastUsedAt: now + day * 4, createdAt: now, updatedAt: now + day * 4,
  },
  {
    id: "p5", title: "翻译助手",
    content: "把下面的内容翻译成英文，保持原意与语气：",
    category: "写作", tags: [], pinned: false, hotkey: "",
    useCount: 1, lastUsedAt: now + day * 2, createdAt: now, updatedAt: now + day * 2,
  },
  {
    id: "p6", title: "会议纪要提炼",
    content: "以下是会议转写文本，请提炼出决议、待办（含责任人与期限）：\n\n{{转写文本}}",
    category: "协作", tags: [], pinned: false, hotkey: "",
    useCount: 2, lastUsedAt: now + day, createdAt: now, updatedAt: now + day,
  },
];

function seed(label: string): ShimSeed {
  return {
    label,
    prompts: JSON.parse(JSON.stringify(prompts)),
    categories: ["开发", "设计", "写作", "协作"],
    importSample: "",
  };
}

async function open(page: Page, label: string) {
  await installShim(page, seed(label));
  await page.goto("/");
}

test.describe("风格前后对比截图", () => {
  test("管理窗口全页面", async ({ page }) => {
    test.setTimeout(60_000);
    await page.setViewportSize({ width: 1040, height: 700 });
    await open(page, "manager");
    await page.waitForSelector(".pitem");
    await page.screenshot({ path: `${DIR}/01-prompts-home.png` });

    await page.locator(".pitem", { hasText: "UI 设计审查" }).click();
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${DIR}/02-editor.png` });

    // 塞两条剪贴板历史再切页（shim 默认空，页面呈现空态）
    await page.evaluate(() => {
      const s = window.__PM_FAKE__.state;
      s.data.clipboard.unshift(
        { id: "c1", content: "这是刚刚复制的一段文本内容，用于演示剪贴板历史的样式。", copiedAt: Date.now() - 60_000, kind: "text" },
        { id: "c2", content: "pnpm build && pnpm test:e2e", copiedAt: Date.now() - 120_000, kind: "text" },
      );
      localStorage.setItem("pm-fake-state", JSON.stringify(s));
    });
    await page.reload();
    await page.waitForSelector(".pitem");
    await page.locator('.nav-btn[title="剪贴板"]').click();
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${DIR}/03-clipboard.png` });

    await page.locator('.nav-btn[title="云同步"]').click();
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${DIR}/04-sync.png` });

    await page.locator('.nav-btn[title="数据"]').click();
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${DIR}/05-data.png` });

    await page.locator('.nav-btn[title="设置"]').click();
    await page.waitForTimeout(150);
    await page.screenshot({ path: `${DIR}/06-settings.png` });

    // 暗色主题下的提示词页
    await page.evaluate(() => {
      const s = window.__PM_FAKE__.state;
      s.data.settings.theme = "dark";
      localStorage.setItem("pm-fake-state", JSON.stringify(s));
    });
    await page.reload();
    await page.waitForSelector(".pitem");
    await page.screenshot({ path: `${DIR}/07-dark-prompts.png` });
  });

  test("快捷面板", async ({ page }) => {
    test.setTimeout(60_000);
    await page.setViewportSize({ width: 760, height: 520 });
    await open(page, "main");
    await page.waitForSelector(".qp-search");
    await page.fill(".qp-search", "设计");
    await page.waitForTimeout(200);
    await page.screenshot({ path: `${DIR}/08-quick-panel.png` });
  });
});
