import { expect, test } from "@playwright/test";

test("新用户进入 Suna 对话中心并可以打开知识中心", async ({ page }) => {
  await page.goto("/");

  await expect(page.locator(".suna-new-sidebar")).toBeVisible();
  await expect(page.getByRole("button", { name: "对话中心" })).toBeVisible();
  await expect(page.getByRole("heading", { name: /你好，我是 Suna/ })).toBeVisible();

  await page.getByRole("button", { name: "知识中心", exact: true }).click();
  await expect(page.getByRole("heading", { name: "知识中心" })).toBeVisible();
  await expect(page.getByRole("button", { name: "新建知识库" })).toBeVisible();
});

test("知识中心在常用窗口尺寸下保持可用布局", async ({ page }) => {
  for (const [width, height] of [[1024, 720], [1440, 900], [1920, 1080]]) {
    await page.setViewportSize({ width, height });
    await page.goto("/");
    await page.getByRole("button", { name: "知识中心", exact: true }).click();
    await expect(page.locator(".suna-knowledge-center")).toBeVisible();

    const geometry = await page.evaluate(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
      bodyHeight: document.body.scrollHeight,
    }));
    expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.clientWidth + 1);
    expect(geometry.bodyHeight).toBeGreaterThan(0);
  }
});
