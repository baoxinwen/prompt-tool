import { test, expect } from "@playwright/test";
import { openApp } from "./fixtures";

test("边界回归：内容溢出时概览卡不被压扁、容器出现滚动", async ({ page }) => {
  // 根因：.sync-body 是固定高可滚动 flex 列，子元素默认 flex-shrink:1 且
  // 概览卡 overflow:hidden 最小高度为 0——内容溢出时卡片先被压扁裁剪，
  // 滚动条永远不出现。契约：卡片保持自然高度，由容器滚动。
  await page.setViewportSize({ width: 1040, height: 610 }); // 默认窗口高度，内容必然溢出
  await openApp(page, "manager");
  await page.locator('.nav-btn[title="云同步"]').click();
  await page.waitForSelector(".ov-desc");
  const r = await page.evaluate(() => {
    const body = document.querySelector(".sync-body")!;
    const ov = document.querySelector(".overview")!;
    const ovRect = ov.getBoundingClientRect();
    const desc = document.querySelector(".ov-desc")!.getBoundingClientRect();
    return {
      bodyClientH: body.clientHeight,
      bodyScrollH: body.scrollHeight,
      overviewH: ovRect.height,
      descInsideCard: desc.bottom <= ovRect.bottom + 0.5,
    };
  });
  expect(r.overviewH).toBeGreaterThanOrEqual(100); // 概览卡保持换行后的自然高度（实测 119；被压扁时仅 34）
  expect(r.descInsideCard).toBe(true); // 描述不被卡片裁剪
  expect(r.bodyScrollH).toBeGreaterThan(r.bodyClientH); // 溢出由容器滚动承担
});
