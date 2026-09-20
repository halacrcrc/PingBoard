import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// 前端入口：挂载 React 根组件
const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("未找到 #root 挂载点");
}

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
