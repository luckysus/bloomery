import { expect, test } from "@playwright/test";

test("new user enters the workbench directly and can start a local conversation", async ({ page }) => {
  await page.addInitScript(() => {
    const settings = new Map<string, string>();
    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextCallbackId = 1;
    let providerConfigured = true;
    let conversationCreated = false;
    let assistantReady = false;

    const conversation = {
      id: "conversation-first-run",
      title: "首次钢铁问题",
      created_at: "2026-08-13T00:00:00Z",
      updated_at: "2026-08-13T00:00:00Z",
      pinned: false,
      archived: false,
    };

    window.__TAURI_INTERNALS__ = {
      transformCallback: (callback: (payload: unknown) => void, once = false) => {
        const id = nextCallbackId++;
        callbacks.set(id, once
          ? (payload) => {
              callbacks.delete(id);
              callback(payload);
            }
          : callback);
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      },
      invoke: async (command: string, args?: Record<string, unknown>) => {
        switch (command) {
          case "plugin:event|listen":
            return 1;
          case "plugin:event|unlisten":
            return null;
          case "db_init":
            return null;
          case "get_setting": {
            const key = String(args?.key ?? "");
            if (settings.has(key)) return settings.get(key);
            if (key === "ui.locale") return JSON.stringify({ version: 1, preference: "zh-CN" });
            return null;
          }
          case "set_setting":
            settings.set(String(args?.key ?? ""), String(args?.valueJson ?? ""));
            return null;
          case "save_provider_profile":
            providerConfigured = true;
            return {
              id: "profile-llm",
              kind: "open_ai_compatible",
              display_name: "OpenAI Compatible",
              base_url: "https://api.example.com/v1",
              model_id: "steel-model",
              enabled: true,
              revision: 1,
              secret_generation: 1,
              secret_configured: true,
            };
          case "secret_set":
            return { configured: true };
          case "test_provider_profile":
            return { ok: true, status_code: 200, error_code: null, elapsed_ms: 5 };
          case "set_default_provider_profile":
            return null;
          case "install_bundled_steel_package":
            return { package: { id: "steel", version: "1.0.0", active: true } };
          case "create_conversation":
            conversationCreated = true;
            return conversation;
          case "list_messages":
            return assistantReady
              ? [
                  {
                    id: "message-first-run-user",
                    conversation_id: conversation.id,
                    role: "user",
                    content: "Q355B 的屈服强度是多少？",
                    response_json: null,
                    created_at: conversation.created_at,
                  },
                  {
                    id: "message-first-run-assistant",
                    conversation_id: conversation.id,
                    role: "agent",
                    content: "请结合钢级、厚度和适用标准核对屈服强度。",
                    response_json: JSON.stringify({ run_id: "run-first-run" }),
                    created_at: conversation.updated_at,
                  },
                ]
              : [];
          case "get_conversation_draft":
            return "";
          case "save_conversation_draft":
            return null;
          case "desktop_agent_chat":
            assistantReady = true;
            return {
              run_id: "run-first-run",
              session_id: conversation.id,
              status: "completed",
              answer: "请结合钢级、厚度和适用标准核对屈服强度。",
            };
          case "list_provider_profiles":
            return providerConfigured
              ? [{
                  id: "profile-llm",
                  kind: "open_ai_compatible",
                  display_name: "OpenAI Compatible",
                  base_url: "https://api.example.com/v1",
                  model_id: "steel-model",
                  enabled: true,
                  revision: 1,
                  secret_generation: 1,
                  secret_configured: true,
                }]
              : [];
          case "list_knowledge_bases":
            return [];
          case "list_conversations":
            return conversationCreated ? [conversation] : [];
          case "list_background_tasks":
            return [];
          case "get_knowledge_health":
            return {
              knowledge_base_count: 0,
              document_count: 0,
              active_document_count: 0,
              version_count: 0,
              chunk_count: 0,
              indexed_chunk_count: 0,
              active_task_count: 0,
            };
          default:
            throw new Error(`Unexpected Tauri command: ${command}`);
        }
      },
    };
  });

  const startedAt = Date.now();
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "工作台" })).toBeVisible();
  await expect(page.getByTestId("workbench-provider-status")).toHaveText("OpenAI Compatible");
  await expect(page.locator("body")).not.toContainText("test-secret-value");

  await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "对话", exact: true }).click();
  await expect(page.getByRole("heading", { name: "对话" })).toBeVisible();
  await page.getByRole("button", { name: "新建对话" }).click();
  await page.getByLabel("输入消息").fill("Q355B 的屈服强度是多少？");
  await page.getByRole("button", { name: "发送" }).click();
  await expect(page.getByText("请结合钢级、厚度和适用标准核对屈服强度。")).toBeVisible();
  expect(Date.now() - startedAt).toBeLessThan(180_000);
});
