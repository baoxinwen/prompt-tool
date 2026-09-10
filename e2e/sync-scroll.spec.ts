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
      overviewScrollH: ov.scrollHeight,
      descInsideCard: desc.bottom <= ovRect.bottom + 0.5,
    };
  });
  // 卡片未被压扁裁剪的精确语义：内容不溢出卡片自身（压扁时
  // scrollHeight 远大于 clientHeight）。绝对像素阈值随环境字体渲染
  // 漂移（本地 119 / CI 98，被压扁时仅 34），只留宽下限兜底
  expect(r.overviewScrollH).toBeLessThanOrEqual(r.overviewH + 0.5);
  expect(r.overviewH).toBeGreaterThanOrEqual(60);
  expect(r.descInsideCard).toBe(true); // 描述不被卡片裁剪
  expect(r.bodyScrollH).toBeGreaterThan(r.bodyClientH); // 溢出由容器滚动承担
});
