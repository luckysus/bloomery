import { expect, test } from "@playwright/test";

test("desktop shell exposes the Suna research workspace", async ({ page }) => {
  await page.goto("/");

  await expect(page.locator(".suna-new-sidebar")).toBeVisible();
  await expect(page.locator(".suna-new-main")).toBeVisible();
  await expect(page.getByText("Suna", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "对话中心" })).toBeVisible();
});

test("desktop shell keeps stable geometry at supported window sizes", async ({ page }, testInfo) => {
  for (const [width, height] of [
    [1024, 720],
    [1440, 900],
    [1920, 1080],
  ]) {
    await page.setViewportSize({ width, height });
    await page.goto("/");
    await expect(page.locator(".suna-new-main")).toBeVisible();

    const geometry = await page.evaluate(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
      bodyHeight: document.body.scrollHeight,
    }));
    expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.clientWidth);
    expect(geometry.bodyHeight).toBeGreaterThan(0);
    await page.screenshot({
      path: testInfo.outputPath(`shell-${width}x${height}.png`),
      fullPage: true,
    });
  }
});

test("navigation remains keyboard reachable and exposes the knowledge center", async ({ page }) => {
  await page.goto("/");

  const knowledge = page.getByRole("button", { name: "知识中心", exact: true });
  await knowledge.focus();
  await expect(knowledge).toBeFocused();
  await knowledge.click();
  await expect(page.getByRole("heading", { name: "知识中心" })).toBeVisible();
});
