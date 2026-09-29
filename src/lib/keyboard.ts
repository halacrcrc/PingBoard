// 键盘交互的纯逻辑：抽离以便单测（tests/frontend_pure.test.mjs），不依赖 DOM / React

/**
 * Delete 键是否应被忽略。
 * 抽成纯函数以便单测：弹层打开时（dialogOpen）或在输入框内打字时必须忽略。
 */
export function shouldIgnoreDeleteKey(targetTag: string | null, dialogOpen: boolean): boolean {
  if (dialogOpen) return true;
  return targetTag === "INPUT" || targetTag === "TEXTAREA";
}
