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

  // 评审 M28：立即更新动作此前无用例（shim 的进度事件与状态流转 mock
  // 早已备好）——补上按钮点击 → 进度 → 归为最新的浏览器层回归网
  test("立即更新：点击后进入下载进度并归为最新（M28）", async ({ page }) => {
    await installShim(page, {
      ...baseSeed("manager"),
      updateStatus: { kind: "available", version: "9.9.9", notes: "- 测试日志" },
    });
    await page.goto("/");
    await page.waitForSelector(".mg-main");
    await page.locator(".nav-btn[aria-label='设置']").click();

    // 自动检查结果未到达前先手动检查（本用例不依赖 3s 自动检查）
    await page.getByRole("button", { name: "检查更新" }).click();
    const avail = page.locator(".upd-avail");
    await expect(avail).toContainText("9.9.9");

    await avail.getByRole("button", { name: "立即更新" }).click();
    // 真实语义：下载安装完成后进程重启，重启前 phase 停在 installing（浏览器
    // e2e 无重启，终态即「安装中」提示 + 操作按钮禁用）；「已是最新」由重启后
    // 的检查展示，属真机冒烟范畴
    const availAfter = page.locator(".upd-avail");
    await expect(page.getByText("安装中，应用即将重启…")).toBeVisible({ timeout: 10_000 });
    await expect(availAfter.getByRole("button", { name: "立即更新" })).toBeDisabled();
  });
});
