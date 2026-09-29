import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 60_000,
  outputDir: "./test-results",
  // CI 上遗留 test.only 会让其余用例静默跳过、门禁形同虚设（评审 M26）
  forbidOnly: !!process.env.CI,
  use: {
    baseURL: "http://127.0.0.1:4173",
    locale: "zh-CN",
    viewport: { width: 900, height: 640 },
  },
  webServer: {
    command: "pnpm exec vite --port 4173 --strictPort --host 127.0.0.1",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
    timeout: 60_000,
  },
  projects: [
    {
      name: "msedge",
      use: { browserName: "chromium", channel: "msedge" },
    },
  ],
  // @shots 截图工具用例默认排除（评审 M27）：显式跑走 pnpm shots
  //（CLI --grep-invert 覆盖本配置）
  grepInvert: /@shots/,
});
