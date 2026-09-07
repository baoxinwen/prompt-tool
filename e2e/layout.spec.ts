import { test, expect } from "@playwright/test";
import { openApp } from "./fixtures";

test("边界回归：设置限宽列在宽窗口下水平居中", async ({ page }) => {
  // 根因：.st-body 有 max-width 但无水平居中，宽窗口下卡片贴左、右侧大片留白
  await page.setViewportSize({ width: 1200, height: 700 });
  await openApp(page, "manager");
  await page.locator('.nav-btn[title="设置"]').click();
  const r = await page.evaluate(() => {
    const cs = getComputedStyle(document.querySelector(".st-body")!);
    return { left: cs.marginLeft, right: cs.marginRight };
  });
  expect(parseFloat(r.left)).toBe(parseFloat(r.right)); // 左右边距相等 = 居中
  expect(parseFloat(r.left)).toBeGreaterThan(50); // 宽窗口下确实产生了居中留白
});

test("边界回归：云同步概览卡收窄时按钮换行、描述保持单行", async ({ page }) => {
  // 根因：概览卡固定三段行式布局（图标+文本+三按钮），卡宽 ≤700 时文本区被压到
  // ~280px，长描述换行出现单字孤行（"播"），观感压缩变形
  await page.setViewportSize({ width: 875, height: 700 });
  await openApp(page, "manager");
  await page.locator('.nav-btn[title="云同步"]').click();
  await page.waitForSelector(".ov-desc");
  const r = await page.evaluate(() => {
    const info = document.querySelector(".ov-info")!.getBoundingClientRect();
    const actions = document.querySelector(".ov-actions")!.getBoundingClientRect();
    const desc = document.querySelector(".ov-desc")!;
    return { wrapped: actions.top >= info.bottom - 1, descH: desc.getBoundingClientRect().height };
  });
  expect(r.wrapped).toBe(true); // 按钮换行到第二行，文本区占满整行
  expect(r.descH).toBeLessThan(24); // 描述单行，无单字孤行
});

test("边界回归：云同步限宽列在宽窗口下水平居中", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await openApp(page, "manager");
  await page.locator('.nav-btn[title="云同步"]').click();
  const r = await page.evaluate(() => {
    const cs = getComputedStyle(document.querySelector(".sync-body")!);
    return { left: cs.marginLeft, right: cs.marginRight };
  });
  expect(parseFloat(r.left)).toBe(parseFloat(r.right));
  expect(parseFloat(r.left)).toBeGreaterThan(50);
});

test("边界回归：数据页限宽列在宽窗口下水平居中", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await openApp(page, "manager");
  await page.locator('.nav-btn[title="数据"]').click();
  const r = await page.evaluate(() => {
    const cs = getComputedStyle(document.querySelector(".de-body")!);
    return { left: cs.marginLeft, right: cs.marginRight };
  });
  expect(parseFloat(r.left)).toBe(parseFloat(r.right));
  expect(parseFloat(r.left)).toBeGreaterThan(50);
});
