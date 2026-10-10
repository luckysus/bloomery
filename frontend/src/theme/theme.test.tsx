import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop, isDesktopRuntime } from "../bridge/desktop";
import { resetSettingsStoreForTests } from "../stores/settingsStore";
import { ThemeProvider, useTheme } from "./theme";

vi.mock("../bridge/desktop", () => ({
  isDesktopRuntime: vi.fn().mockReturnValue(false),
  desktop: {
    getSetting: vi.fn(),
    setSetting: vi.fn(),
  },
}));

function ThemeProbe() {
  const { preference, setPreference } = useTheme();
  return (
    <div>
      <output data-testid="theme-preference">{preference}</output>
      <button type="button" onClick={() => setPreference("dark")}>dark</button>
    </div>
  );
}

describe("ThemeProvider", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetSettingsStoreForTests();
    vi.mocked(desktop.getSetting).mockResolvedValue(null);
    vi.mocked(desktop.setSetting).mockResolvedValue(undefined);
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.removeProperty("color-scheme");
    window.localStorage.removeItem("suna.ui.theme");
  });

  it("loads the saved theme and applies it to the document", async () => {
    vi.mocked(desktop.getSetting).mockResolvedValue(
      JSON.stringify({ version: 1, preference: "dark" }),
    );

    render(
      <ThemeProvider>
        <ThemeProbe />
      </ThemeProvider>,
    );

    await waitFor(() => expect(screen.getByTestId("theme-preference")).toHaveTextContent("dark"));
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("persists a user-selected theme", async () => {
    vi.mocked(isDesktopRuntime).mockReturnValue(true);
    render(
      <ThemeProvider>
        <ThemeProbe />
      </ThemeProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "dark" }));

    await waitFor(() =>
      expect(desktop.setSetting).toHaveBeenCalledWith(
        "ui.theme",
        JSON.stringify({ version: 1, preference: "dark" }),
      ),
    );
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
  });
});
