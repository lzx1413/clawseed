# Multi-Agent Council Mode Implementation Plan

## Context

ClawSeed currently operates as a single-agent system — one `Agent` instance per connection, with full tool loop, memory, and provider. The user wants a "Council Mode" where one Leader agent handles complex tool calls while multiple Reviewer agents provide oversight: reading context, analyzing behavior, and writing feedback to shared Memory. Communication is Memory-based (no new message bus), leveraging the existing `NamespacedMemory` decorator.

### Review-Derived Constraints

Code review of the initial plan identified four structural risks. This version addresses them with three hard constraints:

1. **Council is per-connection, not per-AppState.** The gateway creates a fresh `Council` (Leader + Reviewers) per WebSocket connection, sharing only the underlying `Arc<dyn Provider/Memory/Observer/Tool>` instances — exactly the pattern `from_config_with_shared_components()` already uses. No Agent instance is reused across connections, avoiding history/session/remote-tool leaks.
2. **Reviewer uses explicit tool whitelist, not ReadOnly + exempt_tools.** The reviewer's tool registry only contains read-class tools (`file_read`, `memory_recall`) plus a dedicated `reviewer_memory_store` tool. AutonomyLevel is `Supervised` with `auto_approve` covering all registered tools — no SecurityPolicy holes needed.
3. **Review happens at turn boundary, not mid-loop.** Reviewers run after the Leader's tool loop completes. Mid-loop `request_review` is deferred to a follow-up iteration that requires ToolContext/Agent executor extension — the initial implementation stays out of the turn loop internals.

## Architecture: Council Orchestrator

### Core Flow

```
User Message → Council (per-connection instance)
  → Council reads reviewer feedback from council namespace (shared_memory.recall_namespaced)
  → leader.inject_system_context(feedback)    # Inject as system-role message, NOT user message
  → Leader.turn(original_user_message)         # Full tool loop; auto_save stores clean user msg
  → leader.clear_system_context("[Council]")   # Remove injected system context after turn
  → Council.run_reviewers(leader_history)      # After Leader loop completes
    → Each Reviewer: limited LLM loop (max 3 iterations), reads council namespace, writes feedback
  → Reviewer completion contract: verify review_{role}_* was written
  → Reviewer opinions emitted to user via CouncilStreamEvent  # User sees reviewer feedback too
```

### 1. Council Struct (`clawseed-agent/src/council.rs`)

New file. Council is constructed per-connection, shares Arc components only:

```rust
pub struct Council {
    leader: Agent,                                    // Per-connection, full autonomy
    reviewers: Vec<ReviewerAgent>,                    // Per-connection, restricted tools
    shared_memory: Arc<dyn Memory>,                   // Shared Memory (Leader uses this directly)
    council_namespace: String,                        // e.g. "council_{session_id}"
    config: CouncilConfig,
}

pub struct ReviewerAgent {
    agent: Agent,                                     // Per-connection, Supervised + whitelist
    role: String,                                     // e.g. "security", "quality", "strategy"
    focus_prompt: String,                             // Evaluation focus instruction
}
```

**Construction in gateway** (`clawseed-gateway/src/ws.rs`):

```rust
// Per WebSocket connection — same pattern as current Agent creation
let council = Council::from_shared_components(
    &config,
    state.provider.clone(),           // Arc<dyn Provider> — shared
    state.mem.clone(),                // Arc<dyn Memory> — shared
    state.observer.clone(),           // Arc<dyn Observer> — shared
    state.shared_builtin_tools.clone(), // Arc<[Arc<dyn Tool>] — shared
    session_id,
)?;
// council.leader and council.reviewers are NEW Agent instances,
// sharing the same underlying Provider/Memory/Tool Arc references.
// Leader's agent.memory is the shared Memory directly (no CouncilMemory wrapper).
// Reviewer's agent.memory is NamespacedMemory wrapping the shared Memory.
```

**Construction in CLI** (`clawseed/src/main.rs`):

```rust
// Chat mode: Council creates its own Provider/Memory, no sharing needed
let council = Council::from_config(&config)?;
```

Key methods:
- `Council::from_config(config)` — standalone, creates own Provider/Memory/Observer (CLI)
- `Council::from_shared_components(config, provider, memory, observer, shared_tools, session_id)` — reuses Arc'd instances (gateway)
- `Council::turn(message)` — inject reviewer feedback as system context, run Leader, clear context, run Reviewers
- `Council::turn_streamed(message, event_tx)` — streaming variant with `CouncilStreamEvent` events
- `Council::run_reviewers(leader_history)` — called after Leader loop, runs each Reviewer once

### 2. Config Schema Extensions (`clawseed-config/src/schema/mod.rs`)

Add `CouncilConfig` alongside the existing `AgentEntryConfig` (not replacing it — `AgentEntryConfig` is for named provider API keys, a different purpose):

```toml
[council]
enabled = true
namespace = "council"                   # Memory namespace prefix

[council.leader]
model = ""                              # Optional override, empty = use default

[council.reviewers.security]
role = "security"
focus_prompt = "Evaluate whether tool calls follow security best practices..."
model = "gpt-4o-mini"                   # Can use cheaper/faster model

[council.reviewers.quality]
role = "quality"
focus_prompt = "Evaluate code quality and correctness..."
model = "gpt-4o-mini"
```

Rust structs:

```rust
pub struct CouncilConfig {
    pub enabled: bool,                                    // Default: false
    pub namespace: String,                                // Default: "council"
    pub leader: CouncilLeaderConfig,
    pub reviewers: HashMap<String, CouncilReviewerConfig>, // Default: empty
}

pub struct CouncilLeaderConfig {
    pub model: Option<String>,                            // Override, None = use default
    pub identity_name: Option<String>,                    // Reference to identity config
}

pub struct CouncilReviewerConfig {
    pub role: String,
    pub focus_prompt: String,
    pub model: Option<String>,                            // Override, None = use leader's model
}
```

Reviewer allowed_tools is **not configurable** — it is hardcoded to the reviewer tool set (`file_read`, `memory_recall`, `reviewer_memory_store`). This avoids the SecurityPolicy hole problem entirely.

### 3. Reviewer Agent Construction

Reviewers are `Agent` instances with restricted tool access, built per-connection:

- **AutonomyLevel**: `Supervised` (not ReadOnly — ReadOnly blocks all tool calls including reads)
- **auto_approve**: all registered tools are auto-approved (they're all safe: read + memory write)
- **Tool registry**: only contains three tools:
  - `file_read` (from shared_builtin_tools, filtered by name — same Arc instance reused, no reconstruction)
  - `memory_recall` (from shared_builtin_tools, filtered by name — same Arc instance reused, no reconstruction)
  - `reviewer_memory_store` (new tool, reviewer-specific, constructed per-reviewer)
- **Memory**: `NamespacedMemory::new(shared_memory.clone(), "council_{session_id}")` — the reviewer's `agent.memory` field is this namespaced wrapper, so all store/recall operations are scoped to the council namespace
- **System prompt**: role-specific `focus_prompt` + instruction to write evaluations with key prefix `review_{role}_`
- **Model**: can differ from Leader (e.g. cheaper model per reviewer config)
- **Limited tool loop**: Reviewer `max_tool_iterations` is set to 3 (room for recall + store, not a single-shot call)

**Assembly Constraint**: The reviewer's `DefaultToolRegistry` is NOT a separate full registration system. It is populated by: (1) `register_all_arc()` with filtered `Arc<dyn Tool>` references from `shared_builtin_tools` (selecting `file_read` and `memory_recall` by name — zero construction, zero duplication), then (2) `register_arc()` with the freshly constructed `ReviewerMemoryStoreTool`. No tool instances are duplicated; read-only tools reuse the same Arc references that the Leader uses.

**Immutable Constraint**: `ReviewerMemoryStoreTool` holds `Arc<NamespacedMemory>` (not `Arc<dyn Memory>`) as a compile-time guarantee. Bare shared memory is prohibited — passing `Arc<dyn Memory>` would bypass namespace isolation and write reviewer feedback to the wrong namespace, making it invisible to subsequent recall. The type constraint prevents this at compile time.

The `reviewer_memory_store` tool (`clawseed-tools/src/reviewer_memory_store.rs`):

**Design note**: `ToolContext` currently only provides `workspace_dir()`. The reviewer_memory_store tool needs access to the reviewer's Memory, but extending `ToolContext` with `memory()` is a broader API change (deferred — see Section 11). Instead, the tool holds its own `Arc<NamespacedMemory>` reference, injected at construction.

**Option A (chosen)**: The `ReviewerMemoryStoreTool` holds an `Arc<NamespacedMemory>` (not `Arc<dyn Memory>`) directly, injected at construction. This avoids modifying `ToolContext` — the tool has its own reference to the namespaced memory backend. The concrete type (`Arc<NamespacedMemory>`) enforces the namespace isolation constraint at compile time: bare shared memory cannot be accidentally passed.

**Option B (deferred)**: Extend `ToolContext` with `memory()` method. This is a broader API change that affects all tools. Defer to the mid-loop review iteration.

Chosen implementation:
```rust
pub struct ReviewerMemoryStoreTool {
    role: String,
    council_memory: Arc<NamespacedMemory>,  // Compile-time constraint: must be NamespacedMemory, not bare Arc<dyn Memory>
}

impl Tool for ReviewerMemoryStoreTool {
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let key = format!("review_{}_{}", self.role, args["key"]);
        let content = args["content"].as_str();
        self.council_memory.store(&key, content, Custom("Review"), None)?;
        Ok(ToolResult { success: true, output: "Feedback stored.".into(), error: None })
    }
}
```

This means `ReviewerMemoryStoreTool` is **not a generic built-in tool** — it is constructed per-council with the specific `Arc<NamespacedMemory>` reference. It goes through `clawseed-tools/src/reviewer_registry.rs` via a new `reviewer_tools(role, council_memory, shared_builtin_tools)` function that: (1) filters `shared_builtin_tools` by name to extract `Arc<dyn Tool>` references for `file_read` and `memory_recall` (no reconstruction), (2) constructs one `ReviewerMemoryStoreTool` with the namespaced memory, and (3) returns the combined set as `Vec<Arc<dyn Tool>>`.

### 4. Leader Feedback Consumption

The review identified that using a `CouncilMemory` decorator on Leader's `agent.memory` creates API asymmetry: `recall()` would merge primary+council namespaces, but `get()/forget()/list()` would only operate on primary. This means auto_recall can see reviewer feedback, but explicit memory tools (memory_export, memory_forget) cannot — contradicting NamespacedMemory's principle of "consistently isolate all operations."

A second review identified that the previous fix (prepending reviewer feedback into the user message string) creates context pollution: `Agent::prepare_turn()` wraps the user_message as a `ChatMessage::user()` and pushes it into history, and `auto_save` stores it as `Conversation` memory. Prepending reviewer feedback into the user message means the system treats feedback as if the user said it — polluting both history and auto-saved memory, and bloating round-by-round.

**Solution**: Inject reviewer feedback as a **system-role context block**, not into the user message. This requires two small additions to `Agent`:

```rust
// New public methods on Agent (crates/clawseed-agent/src/agent.rs)
pub fn inject_system_context(&mut self, context: &str) {
    // Insert a system message after the initial system prompt
    // (i.e. after all existing system-role messages, before the first user message)
    let insert_idx = self.history.iter()
        .position(|m| matches!(m, ConversationMessage::Chat(ChatMessage { role, .. }) if role != "system"))
        .unwrap_or(self.history.len());
    self.history.insert(
        insert_idx,
        ConversationMessage::Chat(ChatMessage::system(context)),
    );
}

pub fn clear_system_context(&mut self, marker: &str) {
    // Remove system messages containing the marker (e.g. "[Council]")
    // Preserves the initial system prompt (which doesn't contain the marker)
    self.history.retain(|m| {
        if let ConversationMessage::Chat(msg) = m {
            if msg.role == "system" && msg.content.contains(marker) {
                return false;  // Remove injected context
            }
        }
        true
    });
}
```

**Council flow for each Leader turn**:

```rust
async fn turn(&mut self, message: &str) -> Result<String> {
    // 1. Recall reviewer feedback from council namespace
    let feedback = self.shared_memory
        .recall_namespaced(
            &self.council_namespace,
            "review",          // Broad query to catch all reviewer feedback
            10,                // Limit to recent feedback
            None, None, None, None,
        )
        .await
        .unwrap_or_default();

    // 2. Inject feedback as system context (NOT user message)
    if !feedback.is_empty() {
        let feedback_text = feedback.iter()
            .map(|e| format!("- [{}]: {}", e.key, e.content))
            .collect::<Vec<_>>()
            .join("\n");
        let context = format!(
            "[Council Reviewer Feedback]\n{}\n[/Council Reviewer Feedback]",
            feedback_text
        );
        self.leader.inject_system_context(&context);
    }

    // 3. Run Leader turn with clean user message
    let response = self.leader.turn(message).await?;

    // 4. Clear injected context after turn (prevents history bloat)
    self.leader.clear_system_context("[Council]");

    // 5. Run reviewers
    self.run_reviewers().await?;

    Ok(response)
}
```

This approach:
- **No user-message pollution** — `auto_save` stores the original clean `message` as Conversation memory, not the enriched version with reviewer feedback
- **No history bloat** — injected system context is cleared after each turn, so round N's feedback doesn't persist in round N+1's history (feedback is re-recalled from memory each turn)
- **Semantic correctness** — reviewer feedback is system context, not user input; `ChatMessage::system()` role is correct
- **No API asymmetry** — Leader's `agent.memory` operates identically whether council is enabled or not
- **No new decorator** — eliminates `council_memory.rs` entirely; feedback injection uses a 2-method addition to Agent
- **Consistent with NamespacedMemory** — reviewer isolation is pure `NamespacedMemory`, feedback retrieval is explicit `recall_namespaced` at the orchestrator

### 5. Memory Namespace Conventions

| Key pattern | Written by | Purpose |
|---|---|---|
| `review_{role}_{timestamp}` | Reviewer | Evaluation from reviewer with given role |
| `leader_context_{timestamp}` | Leader (auto-consolidation) | Action summary stored in primary namespace |
| `council_summary_{timestamp}` | Council (orchestrator) | Aggregated feedback summary, stored in council namespace |

All reviewer feedback uses `MemoryCategory::Custom("Review")` in the council namespace.
Leader's own memory stays in its primary namespace with standard categories (Core/Daily/Conversation).

### 6. Reviewer Evaluation Flow

After the Leader's `turn()` or `turn_streamed()` completes, `Council::run_reviewers()`:

1. **Serialize Leader context**: Extract the Leader's completed turn — user message, tool calls made, tool results, final response. Format as a structured summary string.
2. **Store context in council namespace**: `shared_memory.store_with_metadata("leader_context_{ts}", summary, Daily, None, Some(&self.council_namespace), None)` so reviewers can recall it. This uses `store_with_metadata` with the `namespace` parameter to write to the council namespace directly, rather than wrapping in a `NamespacedMemory` decorator.
3. **Run each Reviewer** (sequentially or in parallel, configurable):
   - Reviewer's `agent.turn(review_prompt)` is called with a constructed message: "Review the following actions taken by the Leader agent: {summary}. Write your evaluation using reviewer_memory_store."
   - Reviewer's system prompt already includes `focus_prompt` for role-specific evaluation.
   - Reviewer makes LLM call → may call `memory_recall` to get prior council context → calls `reviewer_memory_store` to write feedback.
   - Reviewer runs a **limited tool loop** (`max_tool_iterations=3`), not a single-shot call — this gives room for recall + store but prevents multi-turn iteration.
4. **Completion contract**: After each reviewer's turn, Council verifies whether `review_{role}_*` keys exist in the council namespace (via `shared_memory.recall_namespaced(&self.council_namespace, "review_{role}", 10, None, None, None, None)` filtered by key prefix). If no feedback was written, Council logs a warning and emits `ReviewCompleted { role, summary: "no feedback written" }` — this prevents silent review failures where the reviewer outputs text but never calls `reviewer_memory_store`.
5. **Reviewer opinions emitted to user**: After each reviewer's completion check, the Council emits `ReviewCompleted` containing the reviewer's evaluation summary. This is the text the reviewer produced (the final assistant message from the reviewer's turn), not just a status marker. The UI surfaces this to the user as supplementary commentary below the Leader's response. The Leader's response remains the primary output; reviewer opinions are labeled and visually subordinate (e.g. indented in CLI, separate styled blocks in Android).

For streaming: `Council::turn_streamed()` sends `CouncilEvent::ReviewStarted/ReviewCompleted` between the Leader's `TurnEvent::Final` and the end of the stream.

### 7. Council Turn Streaming

`CouncilEvent` is emitted alongside `TurnEvent` through the same mpsc channel, using a wrapper enum:

```rust
pub enum CouncilStreamEvent {
    Leader(TurnEvent),
    ReviewStarted { role: String },
    ReviewCompleted { role: String, summary: String },  // summary = reviewer's evaluation text
    ReviewFeedback { role: String, key: String },
}
```

The gateway/CLI receives `CouncilStreamEvent` and dispatches to the UI. Leader events stream during the Leader turn; review events stream after. No interleaving.

**User-visible output**: The user sees both the Leader's response and each reviewer's evaluation. The Leader response is the primary content; reviewer opinions are supplementary, rendered below the Leader response as labeled commentary (e.g. `[Security Review]: No risky tool calls detected` / `[Quality Review]: Suggest refactoring the file_write call`). This gives the user multi-perspective visibility without breaking the single-reply conversation model.

For the CLI: reviewer events are printed as indented lines after the Leader's final response.
For the Android client: reviewer opinions render as separate styled blocks below the Leader's chat bubble, each with the reviewer role as a header.

### 8. Integration Points

**CLI chat mode** (`clawseed/src/main.rs`):
```rust
if config.council.enabled {
    let mut council = Council::from_config(&config)?;
    let (tx, rx) = mpsc::channel(100);
    let response = council.turn_streamed(message, tx, cancel_token, debug).await?;
    // Display Leader response + review events from rx
    // Leader response: printed as normal chat output
    // ReviewStarted/ReviewCompleted: printed as indented lines
    //   e.g. "  [Security Review]: No risky tool calls detected"
    //   e.g. "  [Quality Review]: Suggest refactoring..."
} else {
    // Existing single-agent path unchanged
    let mut agent = Agent::from_config(&config)?;
    // ...
}
```

**Gateway WebSocket** (`clawseed-gateway/src/ws.rs`):
```rust
// Per-connection Council — same lifecycle as current per-connection Agent
if state.council_enabled {
    let council = Council::from_shared_components(
        &config,
        state.provider.clone(),
        state.mem.clone(),
        state.observer.clone(),
        state.shared_builtin_tools.clone(),
        session_id,
    )?;
    // Use council.turn_streamed() instead of agent.turn_streamed()
    // Remote tools: inject into council.leader (same add_remote_tools path)
    // Stream sends CouncilStreamEvent::Leader + ReviewStarted/ReviewCompleted to client
    // Android client renders Leader response as primary bubble,
    // reviewer opinions as separate styled blocks below
}
```

**AppState additions** (`clawseed-gateway/src/lib.rs`):
```rust
pub struct AppState {
    // ... existing fields unchanged
    pub council_available: bool,           // Derived from config; true if reviewers defined
}
```

AppState holds a `council_available` flag derived from config, not an `Arc<Council>` instance. The flag controls whether `set_mode: "council"` requests are accepted. Each connection holds its own `ConnectionHandler` (Agent or Council), constructed or swapped in-place on demand.

### 9. Files to Create/Modify

**New files:**
- `crates/clawseed-agent/src/council.rs` — Council orchestrator, ReviewerAgent, CouncilStreamEvent, per-connection lifecycle, wrap_leader/into_leader for in-place swap
- `crates/clawseed-tools/src/reviewer_memory_store.rs` — reviewer-specific Memory store tool (holds Arc<NamespacedMemory>)
- `crates/clawseed-tools/src/reviewer_registry.rs` — `reviewer_tools(role, council_memory, shared_builtin_tools)` function: filters shared Arc references for file_read/memory_recall by name, constructs ReviewerMemoryStoreTool

**Modified files:**
- `crates/clawseed-config/src/schema/mod.rs` — Add CouncilConfig, CouncilLeaderConfig, CouncilReviewerConfig
- `crates/clawseed-tools/src/registry.rs` — Export reviewer_registry functions
- `crates/clawseed-agent/src/agent.rs` — Add `inject_system_context()` and `clear_system_context()` public methods
- `crates/clawseed-agent/src/mod.rs` — Export council module (no council_memory module needed)
- `crates/clawseed-gateway/src/lib.rs` — Add `council_available` flag to AppState (not `council_enabled`)
- `crates/clawseed-gateway/src/ws.rs` — `ConnectParams.mode`, `ConnectionHandler` enum, `set_mode` message handling, in-place swap, `available_modes` in session_start, `mode_changed` response
- `crates/clawseed-gateway/src/api.rs` — Hot-update `council_available` on config change
- `crates/clawseed/src/main.rs` — Council path when config.council.enabled
- `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/components/ChatBottomBar.kt` — FilterChip mode selector
- `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/ChatViewModel.kt` — Handle mode changes, parse `mode_changed`/`available_modes`, render reviewer opinions
- `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/ChatScreen.kt` — Pass mode state to ChatBottomBar
- `clients/android/sdk/core/src/main/kotlin/dev/clawseed/sdk/core/client/ChatClient.kt` — `setMode()` method, parse server mode responses
- `clients/android/sdk/core/src/main/kotlin/dev/clawseed/sdk/core/DefaultClawSeedSession.kt` — Mode state tracking

**NOT modified** (contrast with initial plan):
- `crates/clawseed-agent/src/security/mod.rs` — No exempt_tools hack; reviewers use Supervised + auto_approve
- `crates/clawseed-api/src/tool_context.rs` — No ToolContext extension; reviewer_memory_store holds its own Arc<NamespacedMemory>

Note: `agent.rs` IS modified in this version (adding `inject_system_context` + `clear_system_context`), but the modification is different from the initial plan's proposal. The initial plan would have added `CouncilEvent` to `TurnEvent` and `CouncilMemory` wrapping to the memory field — this version instead adds two lightweight public methods that don't change the turn loop or memory assignment logic.

### 10. Testing Strategy

1. **Unit tests** (`crates/clawseed-agent/src/council.rs`):
   - Council construction from config (both from_config and from_shared_components)
   - Reviewer tool registry contains only 3 tools (file_read, memory_recall, reviewer_memory_store), all file_read/memory_recall are shared Arc references
   - ReviewerMemoryStoreTool only accepts Arc<NamespacedMemory> (compile-time type check)
   - Reviewer memory operations go to council namespace (NamespacedMemory isolation)
   - Council feedback injection: `inject_system_context` adds system-role ChatMessage with reviewer feedback; `clear_system_context` removes it after turn; user message stays clean (not polluted with reviewer feedback)
   - Completion contract: reviewer that writes no feedback is detected and logged as warning
   - Per-connection isolation: two Council instances with same shared Memory don't leak
   - In-place swap: `wrap_leader` creates Council from existing Agent; `into_leader` extracts Agent back; history preserved in both directions

2. **Integration tests** (`tests/council_integration.rs`):
   - Full council turn: Leader processes message → Reviewers evaluate → feedback stored in council namespace
   - Next Leader turn: `inject_system_context` adds reviewer feedback as system-role message; `clear_system_context` removes it after turn; user message is clean
   - Reviewer opinions are emitted to user via `ReviewCompleted` events (user sees multi-perspective commentary)
   - Multiple reviewers with different roles
   - Completion contract: reviewer that fails to write feedback is detected
   - Council streaming events sequence (Leader events then Review events)
   - In-place swap: WS `set_mode` switches mode mid-session, history preserved across swap
   - `council_available` flag: unavailable council returns error, connection stays single

3. **Config tests** (`crates/clawseed-config`):
   - CouncilConfig TOML parsing with reviewers
   - Default values (enabled=false, empty reviewers, namespace="council")
   - TOML without council section → default CouncilConfig with enabled=false

4. **CI verification**: `./tools/ci_local.sh` must pass after all changes

### 11. Deferred to Follow-Up Iteration

These items are intentionally excluded from the initial implementation and require deeper API changes:

- **Mid-loop `request_review` tool**: Requires extending ToolContext (or Agent executor) to allow a tool to signal the orchestrator and suspend the turn loop. This is a significant API change that should be designed separately once the basic council flow is working.
- **ToolContext `memory()` method**: Adding `memory()` to ToolContext would allow tools to access the agent's memory directly, but this affects all existing tools. Defer until mid-loop review needs it.
- **Reviewer parallel execution**: Initial implementation runs reviewers sequentially. Parallel execution requires careful handling of concurrent Memory writes and streaming event ordering.

### 12. Dynamic Mode Activation

Council Mode is not just a config toggle — it's a **per-connection mode** that can be switched mid-session. The user selects mode via an Android UI selector; the gateway handles in-place swap without reconnection.

#### 12.1 Gateway: `set_mode` WebSocket message

A new WS message type allows mid-session mode switching:

```json
// Client → Server
{"type": "set_mode", "mode": "council"}

// Server → Client (success)
{"type": "mode_changed", "mode": "council", "available_modes": ["single", "council"]}

// Server → Client (council unavailable)
{"type": "error", "message": "Council mode unavailable — no reviewers configured", "code": "COUNCIL_UNAVAILABLE"}
```

#### 12.2 Gateway: `ConnectionHandler` enum for per-connection routing

```rust
enum ConnectionHandler {
    Single(Agent),
    Council(Council),
}
```

**In-place swap logic** — both directions preserve the Leader Agent's history:

- **Single → Council**: `Council::wrap_leader(agent, shared_memory, council_namespace, config)` takes the existing Agent as Leader, constructs Reviewers from shared components, wraps in Council. The Agent instance is reused; no new Agent is created for the Leader role.
- **Council → Single**: `council.into_leader()` extracts the Leader Agent from the Council, drops Reviewers and council namespace references. The returned Agent has the full conversation history intact.

**New methods on Council** (`crates/clawseed-agent/src/council.rs`):
- `Council::wrap_leader(leader, shared_memory, council_namespace, config)` — wraps an existing Agent as Leader
- `Council::into_leader(self)` — extracts Leader Agent, consuming the Council

#### 12.3 Gateway: `available_modes` in `session_start`

The `session_start` message includes which modes are available, so the client can configure its UI:

```json
{
  "type": "session_start",
  "v": 1,
  "session_id": "...",
  "available_modes": ["single", "council"]
}
```

If no reviewers are configured: `available_modes: ["single"]` only.

#### 12.4 Gateway: `council_available` flag in AppState

**File: `crates/clawseed-gateway/src/lib.rs`**

```rust
pub struct AppState {
    // ... existing fields
    pub council_available: bool,  // Derived from config: true if reviewers defined
}
```

Derived from config at startup and updated via `/api/config` PUT hot-update:
- After parsing new config, check if `config.council.reviewers` is non-empty
- Update `state.council_available`
- Lightweight flag update — no Provider/Memory rebuild needed
- Existing Council connections continue running; new connections and `set_mode` attempts use the updated flag

#### 12.5 Gateway: `mode` field in `ConnectParams`

**File: `crates/clawseed-gateway/src/ws.rs`**

```rust
#[derive(Debug, Deserialize)]
struct ConnectParams {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    v: Option<u32>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    /// Agent mode: "single" (default) or "council"
    #[serde(default = "default_mode")]
    mode: String,
}

fn default_mode() -> String { "single".to_string() }
```

When `mode == "council"` in connect and `council_available == true`, the connection starts with Council handler immediately. When `mode == "council"` but unavailable, the gateway falls back to single-agent mode and sends a warning.

#### 12.6 Android: FilterChip mode selector in ChatBottomBar

**File: `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/components/ChatBottomBar.kt`**

Layout:
```
[单代理] [议会]  [OutlinedTextField...] [发送]
```

Implementation:
- New parameters: `currentMode: String`, `availableModes: List<String>`, `onModeChange: (String) -> Unit`
- Two `FilterChip` components: "单代理" (Single) and "议会" (Council)
- Selected chip highlighted; unselected dimmed
- Council chip disabled/greyed if `"council"` not in `availableModes`
- Tapping a chip calls `onModeChange("council")` or `onModeChange("single")`

#### 12.7 Android: ChatViewModel handles mode change

**File: `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/ChatViewModel.kt`**

Mode change flow:
1. User taps [议会] chip → `onModeChange("council")`
2. ViewModel calls `client.setMode("council")` on existing WS connection
3. Gateway performs in-place swap, returns `mode_changed` response
4. ViewModel updates `currentMode = "council"` in UI state
5. On error (`COUNCIL_UNAVAILABLE`): revert chip selection, show snackbar "议会模式不可用"

ViewModel also parses `available_modes` from `session_start` to initialize the mode selector's disabled states.

#### 12.8 Android: ClawseedClient SDK — `setMode` method

**File: `clients/android/sdk/core/src/main/kotlin/dev/clawseed/sdk/core/client/ChatClient.kt`**

```kotlin
fun setMode(mode: String) {
    val msg = buildJsonObject {
        put("type", "set_mode")
        put("mode", mode)
    }
    send(msg.toString())
}
```

Message listener parses `mode_changed` (update local mode state) and `available_modes` from `session_start`.

#### 12.9 Android: Session metadata stores mode

**File: `clients/android/app/src/main/kotlin/dev/clawseed/demo/ui/chat/SessionsViewModel.kt`**

- Session metadata includes `mode: String` (default "single")
- Sessions list shows mode label: "[议会] Security Review Session"
- When resuming a session, `set_mode` is sent after connect to restore the session's mode

## Implementation Order

1. CouncilConfig schema — config foundation
2. ReviewerMemoryStoreTool + reviewer_registry — reviewer-specific tools (Arc<NamespacedMemory> constraint, shared Arc filtering)
3. Council struct + ReviewerAgent — orchestrator core, per-connection lifecycle, system-context injection/clearing, wrap_leader/into_leader for in-place swap
4. Council::turn / turn_streamed — Leader turn + feedback system injection + post-turn review flow
5. Gateway dynamic mode — ConnectionHandler enum, set_mode message, in-place swap, available_modes in session_start, council_available flag, config hot-update, ConnectParams.mode
6. Gateway integration — per-connection routing, remote tool injection into Leader
7. CLI integration — Council::from_config for chat mode
8. Android mode selector — FilterChip in ChatBottomBar, ChatViewModel mode handling, ClawseedClient setMode, session metadata mode
9. Unit + integration tests — validation