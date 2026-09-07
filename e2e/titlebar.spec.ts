import { test, expect } from "@playwright/test";
import { openApp } from "./fixtures";

test("边界回归：最大化时拖拽标题栏先还原再启动拖拽", async ({ page }) => {
  // 根因：data-tauri-drag-region 的 start_dragging 在最大化窗口上不会先还原，
  // 拖拽完全无效果；自管拖拽在最大化时必须先 unmaximize 再 startDragging
  await openApp(page, "manager");
  await page.evaluate(() => {
    window.__PM_FAKE__.state.winMaximized = true;
  });
  await page.locator(".tb").hover();
  await page.mouse.down();
  await page.mouse.move(900, 500);
  await page.mouse.up();
  await page.waitForTimeout(50);
  const calls = await page.evaluate(() => window.__PM_FAKE__.calls());
  expect(calls.win_unmaximize).toBe(1);
  expect(calls.win_start_dragging).toBe(1);
});

test("边界回归：非最大化拖拽标题栏直接启动系统移动", async ({ page }) => {
  await openApp(page, "manager");
  await page.locator(".tb").hover();
  await page.mouse.down();
  await page.mouse.move(900, 500);
  await page.mouse.up();
  await page.waitForTimeout(50);
  const calls = await page.evaluate(() => window.__PM_FAKE__.calls());
  expect(calls.win_unmaximize ?? 0).toBe(0);
  expect(calls.win_start_dragging).toBe(1);
});
