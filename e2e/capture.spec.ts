import { test, expect } from "@playwright/test";
import { openApp } from "./fixtures";

test("边界回归：捕获小窗角落契约（画布与面板同色、圆角对齐 DWM 8px）", async ({ page }) => {
  // 与快捷面板同款结构（无框浮窗 + 自绘圆角边框）：画布底色必须与面板同色、
  // 自绘圆角必须对齐 Win11 DWM 的 8px，否则角落出现双曲线 + 异色楔形。
  await openApp(page, "capture");
  const r = await page.evaluate(() => {
    const cs = (sel: string) => getComputedStyle(document.querySelector(sel)!);
    return {
      bodyBg: cs("body").backgroundColor,
      bodyWindow: document.body.dataset.window,
      cvBg: cs(".cv").backgroundColor,
      cvRadius: cs(".cv").borderRadius,
      cvOverflow: cs(".cv").overflow,
    };
  });
  expect(r.bodyWindow).toBe("capture");
  expect(r.cvOverflow).toBe("hidden");
  expect(r.bodyBg).toBe(r.cvBg); // 契约 1：画布与面板同色，角落无异色楔形
  expect(r.cvRadius).toBe("8px"); // 契约 2：内圈圆角与 DWM 圆角对齐，无双曲线
});
