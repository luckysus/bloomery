# Bloomery Release Plan Coverage

This self-review maps every approved design section to implementation plans. It also records physical plan splits that the roadmap table abbreviates.

| Design section | Implementing plan evidence |
| --- | --- |
| 1-2 Purpose, product, boundaries | foundation Task 10; release-quality Tasks 10-12 |
| 3 Architecture and stack | foundation Tasks 1-9; storage Tasks 1-6; release-quality Tasks 2 and 4 |
| 4 Rust module boundaries | foundation Tasks 2-5; storage Task 2; agent Task 11; release-quality Task 2 |
| 5 Config, secrets, SiliconFlow, MinerU | storage Tasks 3-8 and 11; desktop-management Task 4 |
| 6 Local RAG | local-rag Tasks 1-6; local-rag-indexing Tasks 1-8 |
| 7 Agent runtime and protocol | agent-protocol Tasks 1-11 |
| 8 Tools and permissions | extensions-security Tasks 1-4 and 10; desktop-product Task 5 |
| 9 MCP and Skills | extensions-security Tasks 5-7 and 10; desktop-management Task 3 |
| 10 Domain packages and steel package | extensions-security Tasks 8-10; steel-compute Tasks 1-11 |
| 11 Local data model and migrations | storage Tasks 1-3 and 9; RAG/Agent/extension/steel migrations |
| 12 UI information architecture | desktop-product Tasks 1-6; desktop-management Tasks 1-8 |
| 13 Import, export, backup, diagnostics | desktop-management Tasks 5-6; release-quality Tasks 6 and 12 |
| 14 Error handling and recovery | storage Tasks 5 and 9-11; agent Tasks 3, 8-9; UI failure-state tasks |
| 15 Security and privacy | storage Tasks 4-5; extensions Tasks 2-4 and 6-9; release-quality Tasks 3-4 |
| 16 Tests and performance | every task follows failing-test-first; RAG indexing Task 8; release-quality Tasks 1, 2, 5 |
| 17 Release and open-source operations | release-quality Tasks 4 and 7-12 |
| 18 Upstream reuse | extension Task 5; release-quality Task 4; approved design remains source policy |
| 19 Implementation order | release roadmap Gates A-H and dependency table |
| 20 Formal acceptance | release roadmap Gates A-H; release-quality Task 12 |
| 21 Scale and detailed execution | all plans; no plan establishes a reduced release boundary |

## Physical plan index

1. `2026-07-29-bloomery-release-roadmap.md`
2. `2026-07-29-bloomery-foundation-decoupling.md`
3. `2026-07-29-bloomery-storage-providers.md`
4. `2026-07-29-bloomery-local-rag.md`
5. `2026-07-29-bloomery-local-rag-indexing.md`
6. `2026-07-29-bloomery-agent-protocol.md`
7. `2026-07-29-bloomery-extensions-security.md`
8. `2026-07-29-bloomery-steel-compute.md`
9. `2026-07-29-bloomery-desktop-product.md`
10. `2026-07-29-bloomery-desktop-management.md`
11. `2026-07-29-bloomery-release-quality.md`

## Self-review result

- All 21 design sections have implementation and verification ownership.
- No placeholder tokens or intentionally deferred release features remain.
- Rust Agent orchestration and Python scientific compute boundaries are explicit and non-overlapping.
- RAG and desktop plans are split physically only to keep files maintainable; their paired files share one release gate.
- Current repository policy overrides generic plan advice about commits: execution modifies and verifies the worktree but does not commit without explicit authorization.
