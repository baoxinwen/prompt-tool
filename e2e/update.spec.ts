import { test, expect } from "@playwright/test";
import { installShim } from "./shim";
import { baseSeed } from "./fixtures";

test.describe("更新区", () => {
  test("有新版时：启动自动提醒，设置页展示版本号、日志与操作按钮", async ({ page }) => {
    await installShim(page, {
      ...baseSeed("manager"),
      updateStatus: { kind: "available", version: "9.9.9", notes: "- 测试日志" },
    });
    await page.goto("/");
    await page.waitForSelector(".mg-main");

    // 启动 3s 后自动检查：弹出「发现新版本」通知条，附「去更新」入口
    const toast = page.locator(".toast");
    await expect(toast).toContainText("发现新版本 v9.9.9", { timeout: 10_000 });
    await toast.locator(".toast-action", { hasText: "去更新" }).click();

    // 通知条入口直达设置页；自动检查结果已带入（F3），更新区直接展示版本号、日志与操作按钮，无需二次手动检查
    const avail = page.locator(".upd-avail");
    await expect(avail).toContainText("发现新版本");
    await expect(avail).toContainText("9.9.9");
    await expect(avail).toContainText("- 测试日志");
    await expect(avail.getByRole("button", { name: "立即更新" })).toBeVisible();
    await expect(avail.getByRole("button", { name: "跳过此版本" })).toBeVisible();
    await expect(avail.getByRole("button", { name: "去发布页" })).toBeVisible();
  });

  test("已是最新时：手动检查后显示最新提示", async ({ page }) => {
    await installShim(page, baseSeed("manager"));
    await page.goto("/");
    await page.waitForSelector(".mg-main");
    await page.locator(".nav-btn[aria-label='设置']").click();
    await page.getByRole("button", { name: "检查更新" }).click();
    await expect(page.locator(".upd-now")).toContainText("已是最新");
  });
});
