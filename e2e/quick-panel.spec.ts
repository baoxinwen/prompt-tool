import { test, expect } from "@playwright/test";
import { openApp } from "./fixtures";

test.beforeEach(async ({ page }) => {
  await openApp(page, "main");
});

test("快捷面板启动：列表渲染、首项选中", async ({ page }) => {
  await expect(page.locator(".qp-list .item")).toHaveCount(3);
  await expect(page.locator(".qp-list .item").first()).toHaveClass(/active/);
  await expect(page.locator(".count-all")).toHaveText(/3 项/);
  await page.screenshot({ path: "e2e-artifacts/quick-home.png" });
});

test("拼音首字母搜索命中并回车粘贴", async ({ page }) => {
  await page.locator("input.qp-search").fill("dmsc");
  await expect(page.locator(".qp-list .item")).toHaveCount(1);
  await expect(page.locator(".item-title", { hasText: "代码审查" })).toBeVisible();
  await page.screenshot({ path: "e2e-artifacts/quick-search-pinyin.png" });

  // 带变量的提示词会先弹变量表单，这里改用无变量项验证直贴
  await page.locator("input.qp-search").fill("翻译");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__PM_FAKE__.pastes().length))
    .toBeGreaterThan(0);
  const pastes = await page.evaluate(() => window.__PM_FAKE__.pastes());
  expect(pastes.at(-1)).toMatchObject({ text: /翻译成英文/, promptId: "p3" });
});

test("有变量提示词：回车弹变量表单，填写后粘贴替换结果", async ({ page }) => {
  await page.locator("input.qp-search").fill("周报");
  await page.keyboard.press("Enter");

  const dialog = page.locator(".vd");
  await expect(dialog).toBeVisible();
  const areas = dialog.locator("textarea");
  await expect(areas).toHaveCount(2);
  await areas.nth(0).fill("完成了测试工程化");
  await areas.nth(1).fill("补齐 E2E 覆盖");
  await page.screenshot({ path: "e2e-artifacts/quick-var-dialog.png" });
  await dialog.locator(".vd-foot button").click();

  await expect
    .poll(() => page.evaluate(() => window.__PM_FAKE__.pastes().length))
    .toBeGreaterThan(0);
  const pastes = await page.evaluate(() => window.__PM_FAKE__.pastes());
  expect(pastes.at(-1)!.text).toBe("本周完成：完成了测试工程化，下周计划：补齐 E2E 覆盖");
  expect(pastes.at(-1)!.promptId).toBe("p2");
});

test("Esc 隐藏面板；Tab 切换到剪贴板模式", async ({ page }) => {
  await page.keyboard.press("Escape");
  await expect
    .poll(() => page.evaluate(() => window.__PM_FAKE__.calls().hide_quick ?? 0))
    .toBeGreaterThan(0);

  await page.keyboard.press("Tab");
  await expect(page.locator(".qp-list")).toContainText("暂无剪贴板历史");
  await page.screenshot({ path: "e2e-artifacts/quick-clipboard-empty.png" });
});

test("右上角按钮打开管理窗口命令", async ({ page }) => {
  await page.locator("button.icon-btn[aria-label='打开管理窗口']").click();
  await expect
    .poll(() => page.evaluate(() => window.__PM_FAKE__.calls().open_manager ?? 0))
    .toBeGreaterThan(0);
});

test("边界回归：窗口壳角落契约（画布与面板同色、圆角对齐 DWM 8px）", async ({ page }) => {
  // 根因：快捷面板是无框浮窗，Win11 会给原生窗口加 ~8px DWM 圆角（外圈），
  // 面板 .qp 自绘圆角+边框（内圈）。若画布(body)底色 ≠ 面板底色，或内圈
  // 圆角 ≠ 8px，角落就会出现双曲线 + 异色楔形（用户报告的"内外圆角不一致"）。
  const r = await page.evaluate(() => {
    const cs = (sel: string) => getComputedStyle(document.querySelector(sel)!);
    return {
      bodyBg: cs("body").backgroundColor,
      bodyWindow: document.body.dataset.window,
      qpBg: cs(".qp").backgroundColor,
      qpRadius: cs(".qp").borderRadius,
      qpOverflow: cs(".qp").overflow,
    };
  });
  expect(r.bodyWindow).toBe("main");
  expect(r.qpOverflow).toBe("hidden"); // 内层表面（头部/底部）必须被裁进圆角
  expect(r.bodyBg).toBe(r.qpBg); // 契约 1：画布与面板同色，角落无异色楔形
  expect(r.qpRadius).toBe("8px"); // 契约 2：内圈圆角与 DWM 圆角对齐，无双曲线
});
