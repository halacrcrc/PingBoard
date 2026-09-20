/** @type {import('tailwindcss').Config} */
// Tailwind CSS v3.4 配置（深色模式使用 class 策略，便于手动切换并持久化）
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // 状态色：绿=正常，红=失败（与 PingInfoView 语义一致）
        okgreen: "#16a34a",
        failred: "#dc2626",
      },
      fontFamily: {
        mono: ["Consolas", "Menlo", "Monaco", "monospace"],
      },
    },
  },
  plugins: [],
};
