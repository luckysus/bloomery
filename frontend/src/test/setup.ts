import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

/*
 * Radix UI 原语依赖若干浏览器 API，而 jsdom 未实现它们。
 * 这些是社区标准的测试环境补齐，不改变被测组件的行为：
 * - ResizeObserver：Radix 的 use-size 在挂载时测量元素尺寸。
 * - hasPointerCapture / setPointerCapture / releasePointerCapture：Radix Select、
 *   DropdownMenu 等指针交互组件在打开/关闭时会调用。
 * - scrollIntoView：Radix 在键盘导航时滚动到当前项。
 */
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;
}

/*
 * jsdom 没有实现 PointerEvent。Radix 的 Select / DropdownMenu 依赖
 * `pointerdown` 的 button/ctrlKey 判断来打开浮层；若没有 PointerEvent 构造器，
 * fireEvent.pointerDown 会退化成普通 Event，缺少这些字段，浮层不会打开。
 */
if (typeof window !== "undefined" && !("PointerEvent" in window)) {
  class PointerEventStub extends MouseEvent {
    readonly pointerId = 1;
    readonly pointerType = "mouse";
    readonly isPrimary = true;
    readonly width = 1;
    readonly height = 1;
    readonly pressure = 0.5;
    readonly tiltX = 0;
    readonly tiltY = 0;
    constructor(type: string, params: PointerEventInit = {}) {
      super(type, params);
    }
  }
  (window as unknown as { PointerEvent: unknown }).PointerEvent = PointerEventStub;
  (globalThis as unknown as { PointerEvent: unknown }).PointerEvent = PointerEventStub;
}

if (typeof Element !== "undefined") {
  const proto = Element.prototype as unknown as Record<string, unknown>;
  if (!proto.hasPointerCapture) proto.hasPointerCapture = () => false;
  if (!proto.setPointerCapture) proto.setPointerCapture = () => undefined;
  if (!proto.releasePointerCapture) proto.releasePointerCapture = () => undefined;
  if (!proto.scrollIntoView) proto.scrollIntoView = () => undefined;
}

afterEach(() => cleanup());
