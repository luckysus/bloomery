import { render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AIAnswerRenderer from "./AnswerRenderer";
import { LocaleProvider } from "../../i18n/locale";
import { isDesktopRuntime } from "../../bridge/desktop";

vi.mock("../../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(false),
  desktop: {
    getSetting: vi.fn().mockResolvedValue(JSON.stringify({ preference: "en-US" })),
    setSetting: vi.fn().mockResolvedValue(undefined),
  },
}));

describe("AIAnswerRenderer", () => {
  it("renders English reference and image markers with the selected UI language", async () => {
    vi.mocked(isDesktopRuntime).mockReturnValue(true);
    render(
      <LocaleProvider>
        <AIAnswerRenderer
          answer="Reference 1 and image 1"
          literatureResults={[]}
          imageResults={[]}
        />
      </LocaleProvider>,
    );

    await waitFor(() => {
      const labels = [...document.querySelectorAll(".ref-tag")].map((node) => node.textContent);
      expect(labels).toEqual(expect.arrayContaining(["Reference1", "Image1"]));
    });
  });

  it("renders inline and block math with KaTeX", () => {
    const { container } = render(
      <LocaleProvider>
        <AIAnswerRenderer answer={"质能方程 $E = mc^2$\n\n$$\n\\sigma_y = \\frac{F}{A}\n$$"} literatureResults={[]} />
      </LocaleProvider>,
    );

    expect(container.querySelectorAll(".katex").length).toBeGreaterThanOrEqual(2);
    expect(container.querySelector(".katex-display")).not.toBeNull();
  });

  it("renders GFM tables and fenced code blocks", () => {
    const { container } = render(
      <LocaleProvider>
        <AIAnswerRenderer answer={"| 牌号 | 屈服强度 |\n| --- | --- |\n| Q355B | 355 |\n\n```python\nprint(1)\n```"} literatureResults={[]} />
      </LocaleProvider>,
    );

    expect(container.querySelector("table")).not.toBeNull();
    expect(container.querySelector("code")).not.toBeNull();
  });
});
