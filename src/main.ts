import { createApp } from "vue";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/inter/latin-700.css";
// 标题衬线声部（墨与纸）：只装 600 一档，按 unicode-range 子集按需加载
import "@fontsource/noto-serif-sc/600.css";
import App from "./App.vue";
import "./style.css";

createApp(App).mount("#app");
