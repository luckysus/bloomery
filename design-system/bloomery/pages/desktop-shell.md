# Desktop Shell Override

These rules apply to the Bloomery Tauri desktop shell and override only the
page-level layout details in `MASTER.md`.

## Shell

- Keep the 64px top bar stable across every section.
- Use a 216px expanded navigation rail and a 72px collapsed rail.
- The main canvas is `#F8F7F3`; content groups use warm-white surfaces.
- Keep the shell free of grid textures, heavy gradients, and decorative glow.

## Agent workbench

At desktop widths, the chat route is a bounded three-column work area:

| Column | Responsibility | Default width |
| --- | --- | --- |
| Session rail | Local conversation history and new-session action | 220px |
| Conversation | Messages, citations, and composer | Flexible |
| Inspector | Run state, tools, permissions, tasks, evidence | 286px |

The middle column owns the reading flow. The inspector owns operational
visibility and confirmation actions. On widths below 980px, the inspector
stacks below the conversation; below 640px, all columns become one flow.

## Quality bar

- Do not hide permission controls in an overflow menu.
- Keep citations near the answer and repeat only their compact identifiers in
  the inspector.
- Preserve keyboard focus order: session list → conversation actions →
  messages → composer → inspector actions.
- Do not introduce a new provider, API call, or Web dependency to implement a
  visual change.
