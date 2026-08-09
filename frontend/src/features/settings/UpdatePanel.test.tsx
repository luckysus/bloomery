import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import UpdatePanel from "./UpdatePanel";
import { desktop } from "../../bridge/desktop";

vi.mock("../../i18n/locale", () => ({
  useLocale: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock("../../bridge/desktop", () => ({
  desktop: {
    checkForUpdate: vi.fn(),
    installUpdate: vi.fn(),
  },
}));

describe("UpdatePanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.checkForUpdate).mockResolvedValue(null);
    vi.mocked(desktop.installUpdate).mockResolvedValue(undefined);
  });

  it("shows an available version and installs only after explicit confirmation", async () => {
    vi.mocked(desktop.checkForUpdate).mockResolvedValue({
      version: "0.2.0",
      date: "2026-08-09T08:00:00Z",
      body: "Steel domain improvements",
    });

    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "updateCheck" }));

    expect(await screen.findByText("0.2.0")).toBeInTheDocument();
    expect(desktop.installUpdate).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "updateInstall" }));
    await waitFor(() => expect(desktop.installUpdate).toHaveBeenCalledTimes(1));
  });

  it("reports that the current version is up to date", async () => {
    render(<UpdatePanel />);
    fireEvent.click(screen.getByRole("button", { name: "updateCheck" }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("updateUpToDate"));
  });
});
