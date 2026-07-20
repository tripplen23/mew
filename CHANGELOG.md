# Changelog

## [0.3.2](https://github.com/tripplen23/mew/compare/v0.3.1...v0.3.2) (2026-07-20)


### Features

* **tui+engine:** add directory support to @ mentions ([#67](https://github.com/tripplen23/mew/issues/67)) ([abae5d4](https://github.com/tripplen23/mew/commit/abae5d4949dc6e3cacf3116d9d3beb815ec407b5))

## [0.3.1](https://github.com/tripplen23/mew/compare/v0.3.0...v0.3.1) (2026-07-20)


### Features

* **tui:** redesign entry screen and clear composer leftovers ([#61](https://github.com/tripplen23/mew/issues/61)) ([d57e9fc](https://github.com/tripplen23/mew/commit/d57e9fcf2cb651d264dd65969ddbc2a0ae691c74))

## [0.3.0](https://github.com/tripplen23/mew/compare/v0.2.0...v0.3.0) (2026-07-19)


### Features

* **tui+engine:** Enable '@' command to mention file paths ([#58](https://github.com/tripplen23/mew/issues/58)) ([62b88fa](https://github.com/tripplen23/mew/commit/62b88fa2e6d02589efc4be65b3fd777bd6f44d45))

## [0.2.0](https://github.com/tripplen23/mew/compare/v0.1.0...v0.2.0) (2026-07-18)


### Features

* **tui/engine:** ([#54](https://github.com/tripplen23/mew/issues/54)) ([1419444](https://github.com/tripplen23/mew/commit/14194449a8ce628baf2915af6f608d144e1b7efd))

## 0.1.0 (2026-07-17)


### Features

* add conversation history (Phase 8) and durable memory scaffold (Phase 9) ([d069cc4](https://github.com/tripplen23/mew/commit/d069cc47595e562a84dafc661f7d2063b2521faa))
* add conversation history and durable memory scaffold (Phase 8+9) ([c063171](https://github.com/tripplen23/mew/commit/c0631714b67cb63b6ecfa29c4a39dfc6c84d0b0b))
* add glm-5.2 model + wire memory into chat harness ([01957cf](https://github.com/tripplen23/mew/commit/01957cfecde751279e1218e002a5dd596375f2d0))
* add mew-mcp Go MCP server for external agent integration ([#43](https://github.com/tripplen23/mew/issues/43)) ([018850c](https://github.com/tripplen23/mew/commit/018850c5cbeae72b2ed99246c4f24b71cd73becb))
* **client:** compact one-line tool cards in chat transcript ([#30](https://github.com/tripplen23/mew/issues/30)) ([8c8127a](https://github.com/tripplen23/mew/commit/8c8127aa70eb4f7ce57a862e928872304868e6f3))
* **engine:** Engine v0 ([e8b06be](https://github.com/tripplen23/mew/commit/e8b06bee61b0292496b56a6bd2f51bca9124e25d))
* implement engine v0 harness, tracing, and TUI flow ([d7fec04](https://github.com/tripplen23/mew/commit/d7fec04c99dcb49e3c6498f2048d09263c6c71f1))
* Phase 12 — write_file, edit_file, bash, grep tools + PLAN mode gate + Anthropic prompt caching ([9d5edb2](https://github.com/tripplen23/mew/commit/9d5edb2138f841c359f51a76e0375b8631cc7d9f))
* Phase 9 (memory tool, server routes, CLI) + Phase 10 (streaming) ([b4bf4f8](https://github.com/tripplen23/mew/commit/b4bf4f808816642d2470f5ccd267af7ff3870009))
* **server:** Persistence layer. ([bcb1037](https://github.com/tripplen23/mew/commit/bcb1037a96d7f83788c2088ce308ccdb980c89ff))
* **skills:** 2-tool progressive disclosure + external_dirs + sub-file reads ([bf55f83](https://github.com/tripplen23/mew/commit/bf55f8332323515f49aceb8cac01a8c0f7addd24))
* **tui:** /model and /session slash commands ([#37](https://github.com/tripplen23/mew/issues/37)) ([5710ea5](https://github.com/tripplen23/mew/commit/5710ea5b16c376ad5b782dca99e8ea21cade6b32))
* **tui:** Introduce the TUI ([05413e3](https://github.com/tripplen23/mew/commit/05413e3c55d0b44a2ea0610943aa4454e0b34a60))
* **tui:** Introduce the TUI ([488e2c6](https://github.com/tripplen23/mew/commit/488e2c6b95de3452ffcc1c526644ab183fbc3660))
* **ui:** wire /skills to live catalog, derive /tools from active mod… ([#47](https://github.com/tripplen23/mew/issues/47)) ([8b9c4de](https://github.com/tripplen23/mew/commit/8b9c4de0a7c04da75313c03ddd38c01d73a8ccec))
* wire tool-calling loop — read_file + mewcode_memory e2e ([2d56407](https://github.com/tripplen23/mew/commit/2d564078b4c0fafb5ba1cae0594553e5dab476df))


### Bug Fixes

* address Copilot + CodeRabbit review comments ([1e79e25](https://github.com/tripplen23/mew/commit/1e79e258a7de91e22bf7b5351698125030c99f6b))
* address Copilot and CodeRabbit review comments ([62af30d](https://github.com/tripplen23/mew/commit/62af30dcd404c22a0858ee36a739ee487e008361))
* address Copilot review findings ([afe9f92](https://github.com/tripplen23/mew/commit/afe9f9287b81020a2c43f46ab291fa08b5aad965))
* address CRIT and MAJ issues from review ([252afeb](https://github.com/tripplen23/mew/commit/252afeb6694f405fa7801a71ad87c7fc3910f1b8))
* address PR review findings for engine v0 ([259e8e6](https://github.com/tripplen23/mew/commit/259e8e6b95b169f0fe54f30f1ed88f41b00a73d9))
* **db:** Remove Postgre db for keeping thing simple ([666531d](https://github.com/tripplen23/mew/commit/666531dc83032e11ec30d0dfe176a774ee06ac00))
* harden memory tool + clippy-clean streaming API ([5ee3610](https://github.com/tripplen23/mew/commit/5ee3610c7d847a2f643404b5c200d0e21834f59d))
* include system prompt in Langfuse-visible input ([d28d29c](https://github.com/tripplen23/mew/commit/d28d29cdaa830fe01973c025cd618f47a01ad6f6))
* **phase12:** address CodeRabbit review — code + test gaps ([24075b5](https://github.com/tripplen23/mew/commit/24075b58921d49f1d75495ef5540e12a447c98be))
* **phase12:** provide memory store in plan_mode_filters_write_tools test ([fc8caa7](https://github.com/tripplen23/mew/commit/fc8caa7e9bf569b1c98c96e555a8ef9128a776ef))
* **ponytail:** Esc closes overlay before navigating Home; Left/Right cursor in title; toast Unicode width ([4cbcf0a](https://github.com/tripplen23/mew/commit/4cbcf0a222ebbd359cc9f267977c91661fcc6825))
* populate Langfuse trace IO fields ([0def986](https://github.com/tripplen23/mew/commit/0def98616cb3a12c0ea760a7ca46c1e45e35a36c))
* remove stale Phase 10 reference in read_file tool description ([c5e41ac](https://github.com/tripplen23/mew/commit/c5e41ac36aa0f362efd7c785348273657c05f953))
* route OpenCode Go chat completions through Rig ([0ebca59](https://github.com/tripplen23/mew/commit/0ebca5922a113bb2ce51742af0a299723fd27e05))
* **skills:** address CodeRabbit review — project-shadows-global, SKILL.md rejection, asset prefix ([38eab56](https://github.com/tripplen23/mew/commit/38eab561d2e90691b802f860a1e5230d02cd95fa))
* **skills:** address PR [#11](https://github.com/tripplen23/mew/issues/11) review — bug + hygiene pass ([59b9229](https://github.com/tripplen23/mew/commit/59b9229169421c850bd520b39ab9f44c248561b8))
* **trace:** put user message first in observation input ([7455763](https://github.com/tripplen23/mew/commit/7455763ddc3fcd7efd1db084badb07ea2c9b64a8))
* **tui:** Add MAX_INPUT_HEIGHT for chat input and fix the QUIT_COMMAND ([#36](https://github.com/tripplen23/mew/issues/36)) ([6fcebd4](https://github.com/tripplen23/mew/commit/6fcebd4db85085043511f3408fa2ce7389befe17))
* update agent-pattern regression test for agent/ refactor ([a9dfbec](https://github.com/tripplen23/mew/commit/a9dfbec0fe1f39ec2a86a49c5fb4be4a4a771370))
* use LANGFUSE_BASE_URL only ([01575da](https://github.com/tripplen23/mew/commit/01575daa39d46af2a8b41bc8c29d68de8c35f542))
