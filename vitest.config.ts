import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";
import pkg from "./package.json";

export default defineConfig({
  // 与 vite.config.ts 一致：组件里引用的版本号常量在测试环境同样可用
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  test: {
    projects: [
      {
        plugins: [vue()],
        define: { __APP_VERSION__: JSON.stringify(pkg.version) },
        test: {
          name: "lib",
          environment: "node",
          include: ["src/lib/*.test.ts"],
        },
      },
      {
        plugins: [vue()],
        define: { __APP_VERSION__: JSON.stringify(pkg.version) },
        test: {
          name: "components",
          environment: "happy-dom",
          include: ["src/components/**/*.test.ts", "src/views/**/*.test.ts"],
        },
      },
    ],
  },
});
