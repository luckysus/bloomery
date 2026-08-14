# Bloomery UI Design System

Bloomery is a Windows-first, local-first steel-agent workbench. The desktop
interface should feel like the Web application family: warm white surfaces,
quiet borders, readable typography, and steel blue for the primary action.
Industrial identity comes from restrained steel-blue and amber status accents,
not from dark control-room decoration.

## Product language

- Calm, traceable, and useful for long working sessions.
- White and warm-neutral canvas with steel-blue navigation and actions.
- Use amber for attention and permission decisions; use green for healthy
  runtime and retrieval evidence.
- Keep the primary workflow visible: conversation, context, tools, and proof.

## Tokens

| Role | Token | Value |
| --- | --- | --- |
| Canvas | `--bloomery-bg` | `#FAF9F5` |
| Raised surface | `--bloomery-bg-raised` | `#FFFDF9` |
| Soft surface | `--bloomery-bg-soft` | `#F5F0E8` |
| Sidebar | `--bloomery-sidebar` | `#F7F4EF` |
| Border | `--bloomery-line` | `#EADFD2` |
| Strong border | `--bloomery-line-strong` | `#D8CDBC` |
| Primary text | `--bloomery-text` | `#2B2118` |
| Secondary text | `--bloomery-text-soft` | `#5B5048` |
| Muted text | `--bloomery-text-muted` | `#8A7665` |
| Steel action | `--bloomery-steel` | `#557684` |
| Attention | `--bloomery-amber` | `#B85F43` |
| Healthy state | `--bloomery-green` | `#3F8B75` |

## Typography

- Latin UI: Inter.
- Simplified Chinese UI: Noto Sans SC.
- Tool names, paths, identifiers, and numeric traces: JetBrains Mono.
- Body text stays at 14–16px with 1.5–1.75 line height.
- Use 600–700 weights for headings and 400–500 for body copy.

## Layout

- Global shell: 64px top bar, collapsible left navigation, one main scroll
  region.
- Main content uses a 4/8px rhythm with 16px, 24px, and 32px hierarchy steps.
- Use cards for meaningful groups, not every individual row.
- The chat workbench is three columns on desktop:
  1. session history;
  2. conversation and composer;
  3. runtime, tools, permission, task, and evidence inspector.
- At narrow widths the inspector stacks below the conversation, then the
  session list becomes the first section.

## Interaction

- Use Lucide SVG icons consistently; never use emoji as structural controls.
- Keep focus rings visible and icon-only buttons labelled.
- Use 150–300ms transitions without layout-shifting hover effects.
- Every loading, failure, permission, and task state has text plus an icon.
- Respect `prefers-reduced-motion`.

## Component treatment

- Cards: warm-white surface, 1px border, 12–16px radius, soft shadow.
- Primary buttons: steel blue background with white text.
- Secondary buttons: white background with quiet warm-gray border.
- Inputs: white or warm-gray surface, 9–12px radius, steel-blue focus ring.
- Empty states explain what to do next and never compete with the main action.

