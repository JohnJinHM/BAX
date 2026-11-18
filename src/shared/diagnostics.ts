// 把前端的错误交给 Rust 写进诊断日志。发布版没有开发者工具，不这样做就看不到界面里出的错（ADR-0007）。
// 只上报错误本身：消息和调用栈，不附带快照或界面上的数据。条数由 Rust 一侧限流。

import type { App } from "vue";

import { commands } from "./engine";

function send(kind: string, error: unknown) {
  const message =
    error instanceof Error ? `${error.name}: ${error.message}` : typeof error === "string" ? error : String(error);
  const stack = error instanceof Error && error.stack ? error.stack : null;
  // 上报失败就算了：这里再抛出会变成新的未处理拒绝
  commands.reportUiError(kind, message, stack).catch(() => undefined);
}

/** 接管 Vue 的错误处理、未捕获的异常和未处理的 Promise 拒绝。开发时同时打印到控制台。 */
export function reportErrors(app: App) {
  app.config.errorHandler = (error, _instance, info) => {
    console.error(error);
    send(`vue:${info}`, error);
  };
  window.addEventListener("error", (event) => {
    send("error", event.error ?? event.message);
  });
  window.addEventListener("unhandledrejection", (event) => {
    send("rejection", event.reason);
  });
}
