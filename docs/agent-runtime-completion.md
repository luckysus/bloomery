# Agent Runtime Completion

Agent runtime controls are available in the desktop application:

1. Long-running work can continue in the background and resume from saved checkpoints.
2. Cron plans have an entry under Management > Agents. They trigger while the desktop client is running and deliver results to the selected conversation; reopening the client resumes pending schedules.
3. The same Agent page edits each expert's prompt, Provider, exact tool allowlist, permission restrictions, and execution budgets. Experts cannot exceed the parent Agent's permissions or remaining limits.
4. The chat run inspector lists child tasks, replays stored events, shows live progress and results, and sends cancellation requests.
5. Agents can read and write files and run PowerShell within the configured working directory. A blank directory uses the app data `agent-workspace` directory.
6. Skills can be disabled from Management > Skills. Network retries and checkpoint recovery attempts are separate settings; both default to 2.
7. The default context budget is 32,768 tokens. A model request defaults to a 120-second timeout, while a whole Agent run defaults to 1,800 seconds.

Validation covers Rust formatting, the complete Rust test suite, TypeScript, runtime-boundary validation, the complete frontend test suite, and the production frontend build.
