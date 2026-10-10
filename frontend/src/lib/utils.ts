import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * shadcn/ui 组件使用的类名合并工具。
 *
 * `clsx` 负责条件类名，`tailwind-merge` 负责消解冲突的 Tailwind 工具类
 * （例如传入的 `px-4` 覆盖组件默认的 `px-2`）。
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
