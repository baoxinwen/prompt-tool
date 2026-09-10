import { createApp } from "vue";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-700.css";
// 标题衬线声部（墨与纸）：只装 600 一档，按 unicode-range 子集按需加载
import "@fontsource/noto-serif-sc/600.css";
import App from "./App.vue";
import "./style.css";

const app = createApp(App);
// 全局错误兜底（评审 2026-09-10 M-views#10）：渲染期异常至少落一条结构化日志，
// 桌面用户无 devtools，静默吞掉会让问题完全不可诊断
app.config.errorHandler = (err, _instance, info) => {
  console.error("[prompt-tool] 渲染错误:", info, err);
};
app.mount("#app");
