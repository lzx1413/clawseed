# CLAUDE.md

This file is the short operating guide for Claude Code in this repository. Keep detailed architecture and module documentation in `docs/`; use this file only for commands, conventions, and pointers.

## Priority Rules

- Start every user-facing reply with `Developer`.
- Before starting development work, write a short plan and ask the user to confirm it. Do not begin implementation until the user confirms.
- If unclear or risky details appear during development, stop and ask the user before proceeding.
- Use detailed Angular-style commit messages: `type(scope): subject`. Include a commit body for non-trivial changes explaining what changed, why it changed, and how it was verified.

## Common Commands

```bash
cargo build                          # Debug build
cargo build --release                # Optimized release build
cargo check                          # Fast workspace type-check
cargo clippy                         # Lint
cargo test                           # All tests
cargo test -p clawseed-agent         # Single crate
cargo test --test agent_integration  # Single integration test target
cargo fmt                            # Format
./tools/ci-local.sh                  # Pre-commit local CI
```

Run the gateway:

```bash
./target/release/clawseed gateway --host 0.0.0.0 --port 3000
```

Run local interactive chat:

```bash
./target/release/clawseed chat
./target/release/clawseed chat --model gpt-4o --temperature 0.5
./target/release/clawseed chat --system-prompt "You are..."
```

Android demo:

```bash
./tools/build-clawseed-android.sh aarch64 build
cd clients/android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## Project Facts

- Rust edition 2024, minimum Rust version 1.95.
- Config defaults to `~/.clawseed/clawseed.toml`; workspace defaults to `~/.clawseed/workspace/`.
- Workspace crates: `clawseed-api`, `clawseed-agent`, `clawseed-tools`, `clawseed-providers`, `clawseed-memory`, `clawseed-config`, `clawseed-gateway`, `clawseed`.
- Android client lives in `clients/android/`.
- Release profile uses fat LTO, `codegen-units = 1`, `strip = true`, `panic = "abort"`.

## Where To Read

- Overview and quick start: `README.md` / `README_zh.md`
- Architecture and runtime init chain: `docs/en/architecture.md` / `docs/zh/architecture.md`
- Build, test, Android cross-compilation: `docs/en/build-and-test.md` / `docs/zh/build-and-test.md`
- Module docs:
  - Agent: `docs/en/modules/agent.md` / `docs/zh/modules/agent.md`
  - API traits: `docs/en/modules/api.md` / `docs/zh/modules/api.md`
  - Config: `docs/en/modules/config.md` / `docs/zh/modules/config.md`
  - Gateway: `docs/en/modules/gateway.md` / `docs/zh/modules/gateway.md`
  - Memory: `docs/en/modules/memory.md` / `docs/zh/modules/memory.md`
  - Providers: `docs/en/modules/providers.md` / `docs/zh/modules/providers.md`
  - Tools: `docs/en/modules/tools.md` / `docs/zh/modules/tools.md`
- Remote tools: `docs/en/remote-tool-call.md` / `docs/zh/remote-tool-call.md`
- CETP external tool protocol: `docs/en/external-tool-protocol.md` / `docs/zh/external-tool-protocol.md`
- Android demo architecture: `docs/en/android-demo.md` / `docs/zh/android-demo.md`
- Skills: `docs/en/skills.md` / `docs/zh/skills.md`

## Architecture Notes

- ClawSeed is a Rust AI agent runtime with trait-based extension points.
- Runtime dependency flow is one-way: `clawseed-api <- agent <- tools/providers/memory <- gateway <- binary`.
- `Agent::from_config_with_registry()` is used for CLI/embedded assembly and creates provider, memory, tools, hooks, and dispatcher from config.
- `Agent::from_config_with_shared_components()` is used by gateway paths and reuses shared `AppState` provider, memory, observer, model, temperature, and BuiltIn tool instances.
- Gateway has two tool registries: shared `AppState.tool_registry` for `/api/tools`, and per-agent `Agent.tool_registry` for actual dispatch.
- Remote tools must be visible in the shared registry and injected into the per-connection agent before use.
- MCP is not a usable capability yet. `ToolSource::Mcp` and config/schema/filtering exist, but MCP client types are stubs.

## Conventions

- Before commits, run `./tools/ci-local.sh` and fix failures.
- Keep commit subjects imperative, lowercase unless naming code, and usually under 72 chars.
- Use `!` plus a `BREAKING CHANGE:` footer for breaking changes.
- Prefer existing crate boundaries and helper APIs over new abstractions.
- Disabled tools should not register; missing memory should degrade to `NoneMemory`.
- Hook pipeline supports before/after tool execution; `SecurityPolicy` is the first hook.
