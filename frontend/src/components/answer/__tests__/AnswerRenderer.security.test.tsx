import { render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AIAnswerRenderer from "../AnswerRenderer";
import { proxyImg } from "../../../utils/searchRender";
import { LocaleProvider } from "../../../i18n/locale";

vi.mock("../../../bridge/desktop", () => ({
  desktop: {
    getSetting: vi.fn().mockResolvedValue(JSON.stringify({ preference: "zh-CN" })),
    setSetting: vi.fn().mockResolvedValue(undefined),
  },
}));

function renderAnswer(answer: string) {
  return render(
    <LocaleProvider>
      <AIAnswerRenderer answer={answer} literatureResults={[]} imageResults={[]} />
    </LocaleProvider>,
  );
}

describe("AIAnswerRenderer rendering safety", () => {
  it("neutralizes javascript: scheme in Markdown links", async () => {
    const { container } = renderAnswer("[x](javascript:alert(1))");

    await waitFor(() => {
      expect(container.querySelector("a")).not.toBeNull();
    });

    const anchor = container.querySelector("a")!;
    const href = (anchor.getAttribute("href") || "").toLowerCase();
    expect(href).not.toContain("javascript:");
  });

  it("neutralizes data:text/html scheme in Markdown images", async () => {
    const { container } = renderAnswer("![x](data:text/html,<h1>hi</h1>)");

    await waitFor(() => {
      expect(container.querySelector("img")).not.toBeNull();
    });

    const img = container.querySelector("img")!;
    const src = (img.getAttribute("src") || "").toLowerCase();
    expect(src.startsWith("data:")).toBe(false);
  });

  it("does not render injected <script> tags", async () => {
    const { container } = renderAnswer("Hello <script>alert(1)</script> world");

    await waitFor(() => {
      expect(container.textContent).toContain("Hello");
    });

    expect(container.querySelector("script")).toBeNull();
  });

  it("does not render injected <iframe> tags", async () => {
    const { container } = renderAnswer('Hello <iframe src="https://evil.example.com"></iframe> world');

    await waitFor(() => {
      expect(container.textContent).toContain("Hello");
    });

    expect(container.querySelector("iframe")).toBeNull();
  });
});

describe("proxyImg scheme allow-list", () => {
  it("passes through http/https and relative/local resource paths", () => {
    expect(proxyImg("https://cdn.example.com/a.png")).toBe("https://cdn.example.com/a.png");
    expect(proxyImg("http://cdn.example.com/a.png")).toBe("http://cdn.example.com/a.png");
    expect(proxyImg("/local/asset.png")).toBe("/local/asset.png");
    expect(proxyImg("./thumb.png")).toBe("./thumb.png");
    expect(proxyImg("images/metallography/1.png")).toBe("images/metallography/1.png");
  });

  it("rejects javascript:/data:/vbscript: schemes", () => {
    expect(proxyImg("javascript:alert(1)")).toBe("");
    expect(proxyImg("data:text/html,<script>alert(1)</script>")).toBe("");
    expect(proxyImg("data:image/png;base64,AAAA")).toBe("");
    expect(proxyImg("vbscript:msgbox(1)")).toBe("");
    expect(proxyImg("  JavaScript:alert(1)")).toBe("");
  });
});
