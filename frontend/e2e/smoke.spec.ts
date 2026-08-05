import { expect, test } from "@playwright/test";

test("desktop shell exposes the local workbench", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("main", { name: "工作台" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "工作台" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "主导航" })).toBeVisible();
});

test("desktop shell keeps stable geometry at supported window sizes", async ({ page }, testInfo) => {
  for (const [width, height] of [
    [1024, 720],
    [1440, 900],
    [1920, 1080],
  ]) {
    await page.setViewportSize({ width, height });
    await page.goto("/");
    await expect(page.getByRole("main", { name: "工作台" })).toBeVisible();

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

test("navigation remains keyboard reachable and collapsible", async ({ page }) => {
  await page.goto("/");

  const toggle = page.getByRole("button", { name: "折叠侧栏" });
  await toggle.focus();
  await expect(toggle).toBeFocused();
  await toggle.click();
  await expect(page.getByRole("button", { name: "展开侧栏" })).toBeFocused();
  await expect(page.getByRole("button", { name: "知识库" })).toHaveAttribute("title", "知识库");
});
