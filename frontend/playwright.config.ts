import { defineConfig } from "@playwright/test";

const executablePath = process.env.BLOOMERY_PLAYWRIGHT_EXECUTABLE;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  use: {
    baseURL: "http://127.0.0.1:4173",
    locale: "zh-CN",
    trace: "retain-on-failure",
    reducedMotion: "reduce",
    ...(executablePath ? { launchOptions: { executablePath } } : {}),
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 4173",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
