import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const root = process.cwd();
const logicPath = path.join(root, "src", "components", "agent", "agentTurnNavigator.ts");
const componentPath = path.join(root, "src", "components", "agent", "AgentTurnNavigator.tsx");
const panelPath = path.join(root, "src", "components", "agent", "AgentChatPanel.tsx");

assert.ok(fs.existsSync(logicPath), "agent turn navigation logic must exist");

const logicSource = fs.readFileSync(logicPath, "utf8");
const compiled = ts.transpileModule(logicSource, {
  compilerOptions: {
    module: ts.ModuleKind.ES2020,
    target: ts.ScriptTarget.ES2020,
  },
}).outputText;
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`;
const {
  buildUserTurns,
  calculateTurnRailTranslate,
  calculateTurnWaveWidth,
  getRevealedTurnIndex,
  getActiveTurnIndex,
} = await import(moduleUrl);

assert.deepEqual(
  buildUserTurns([
    { role: "agent", content: "回答 A" },
    { role: "user", content: "  第一个问题  " },
    { role: "user", content: "   " },
    { role: "agent", content: "回答 B" },
    { role: "user", content: "第二个问题" },
  ]),
  [
    { messageIndex: 1, question: "第一个问题" },
    { messageIndex: 4, question: "第二个问题" },
  ],
  "only non-empty raw user questions should become turns",
);

assert.equal(getActiveTurnIndex([], 0, 600), -1);
assert.equal(getActiveTurnIndex([40, 240, 520], 0, 600), 0);
assert.equal(getActiveTurnIndex([40, 240, 520], 100, 600), 1);
assert.equal(getActiveTurnIndex([40, 240, 520], 500, 600), 2);

assert.equal(calculateTurnRailTranslate({
  activeIndex: 0,
  itemCount: 2,
  itemHeight: 10,
  viewportHeight: 100,
  edgePadding: 10,
}), 40, "a short rail should be centered");
assert.equal(calculateTurnRailTranslate({
  activeIndex: 0,
  itemCount: 10,
  itemHeight: 10,
  viewportHeight: 60,
  edgePadding: 10,
}), 10, "the first item should enter below the top fade");
assert.equal(calculateTurnRailTranslate({
  activeIndex: 5,
  itemCount: 10,
  itemHeight: 10,
  viewportHeight: 60,
  edgePadding: 10,
}), -25, "a middle active item should stay centered");
assert.equal(calculateTurnRailTranslate({
  activeIndex: 9,
  itemCount: 10,
  itemHeight: 10,
  viewportHeight: 60,
  edgePadding: 10,
}), -50, "the last item should enter above the bottom fade");

assert.equal(
  typeof calculateTurnWaveWidth,
  "function",
  "turn wave width helper must exist",
);
assert.deepEqual(
  Array.from({ length: 5 }, (_, index) => calculateTurnWaveWidth(index, null)),
  [8, 8, 8, 8, 8],
  "idle turn lines must all have the same width",
);
assert.deepEqual(
  Array.from({ length: 9 }, (_, index) => calculateTurnWaveWidth(index, 4)),
  [8, 12, 16, 24, 32, 24, 16, 12, 8],
  "hovered turn lines must form a stepped wave",
);
assert.equal(getRevealedTurnIndex(null, null), null);
assert.equal(getRevealedTurnIndex(3, 1), 1, "keyboard focus must remain visible over hover");
assert.equal(getRevealedTurnIndex(3, null), 3, "hover must drive the wave without keyboard focus");
assert.equal(getRevealedTurnIndex(null, 1), 1, "keyboard focus must drive the wave without hover");

assert.ok(fs.existsSync(componentPath), "AgentTurnNavigator component must exist");
const component = fs.readFileSync(componentPath, "utf8");
const panel = fs.readFileSync(panelPath, "utf8");

assert.match(component, /buildUserTurns\(messages\)/);
assert.match(component, /maskImage:\s*"linear-gradient\(/);
assert.match(component, /WebkitMaskImage:\s*"linear-gradient\(/);
assert.match(component, /max-md:hidden/);
assert.match(component, /aria-label="本次对话问题导航"/);
assert.match(component, /aria-current=\{active \? "location" : undefined\}/);
assert.match(component, /\{turn\.question\}/, "hover content must show the raw user question");
assert.match(component, /prefers-reduced-motion/);
assert.match(component, /scrollContainer\.scrollTo\(/);
assert.match(component, /TURN_ITEM_HEIGHT\s*=\s*24/, "turn targets need a stable 24px hit height");
assert.match(component, /RAIL_EDGE_PADDING\s*=\s*44/, "rail bounds must match the visual fade");
assert.match(component, /black 44px, black calc\(100% - 44px\)/, "active edge turns must clear the fade");
assert.match(component, /scrollContainer\.firstElementChild/, "content height changes must refresh turn offsets");
assert.match(component, /className="absolute inset-y-0 left-3 w-12 overflow-hidden"/);
assert.match(component, /style=\{\{\s*width:\s*calculateTurnWaveWidth\(index,\s*revealedIndex\)\s*\}\}/);
assert.match(component, /items-center justify-start outline-none/, "turn lines must grow right from one fixed left edge");
assert.match(component, /const revealedIndex = getRevealedTurnIndex\(hoveredIndex, focusedIndex\)/);
assert.match(component, /activeIndex:\s*focusedIndex \?\? activeIndex/, "keyboard focus must bring clipped turns into view");
assert.match(component, /onMouseLeave=\{\(\) => setHoveredIndex\(null\)\}/);
assert.match(component, /event\.currentTarget\.matches\(":focus-visible"\)/);
assert.match(component, /onBlur=\{\(\) => setFocusedIndex\(null\)\}/);
assert.match(component, /onClick=\{\(\) => scrollToTurn\(turn\.messageIndex,\s*index\)\}/);
assert.match(component, /className="absolute left-14 z-30/);
assert.match(component, /role="tooltip"/);
assert.match(component, /aria-describedby=\{/);
assert.match(panel, /<AgentTurnNavigator[\s\S]*messages=\{messages\}[\s\S]*scrollContainerRef=\{messagesRef\}/);
assert.match(panel, /data-agent-user-turn=\{index\}/);
assert.match(panel, /\[--agent-turn-gutter:clamp\(/);
assert.match(panel, /max-md:\[--agent-turn-gutter:0px\]/);
assert.ok(
  (panel.match(/grid-cols-\[var\(--agent-turn-gutter\)_minmax\(0,1fr\)_var\(--agent-turn-gutter\)\]/g) || []).length >= 2,
  "messages and composer must use the same symmetric desktop gutter",
);
assert.match(panel, /max-md:grid-cols-1/);

console.log("agent turn navigator self-test passed");
