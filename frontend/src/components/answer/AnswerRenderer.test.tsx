import { render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AIAnswerRenderer from "./AnswerRenderer";
import { LocaleProvider } from "../../i18n/locale";

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    getSetting: vi.fn().mockResolvedValue(JSON.stringify({ preference: "en-US" })),
    setSetting: vi.fn().mockResolvedValue(undefined),
  },
}));

describe("AIAnswerRenderer", () => {
  it("renders English reference and image markers with the selected UI language", async () => {
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
});
