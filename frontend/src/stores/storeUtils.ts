/**
 * Zustand store 的公共工具。
 *
 * 迁移前的状态由 React `useState` 持有，其 setter 既接受值也接受
 * `(previous) => next` 形式的更新函数。为了让 store 的 setter 与之一一对应、
 * 消费方无需改动，这里提供同语义的更新器解析函数。
 */
export type Updater<T> = T | ((previous: T) => T);

export function resolveUpdater<T>(next: Updater<T>, previous: T): T {
  return typeof next === "function" ? (next as (value: T) => T)(previous) : next;
}
