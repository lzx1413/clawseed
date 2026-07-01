//! Persona resolution — derive a per-connection config/memory override from a
//! named entry in `config.agents`.
//!
//! A persona is a named bundle of overrides: soul (`identity` or
//! `system_prompt`), memory isolation (`memory_namespace`), and tool filters
//! (`allowed_tools`/`denied_tools`). Resolution clones the global `Config`,
//! applies the overrides, and optionally wraps the shared memory backend in a
//! [`NamespacedMemory`]. The resulting config + memory are then fed into the
//! existing `Agent::from_config_with_shared_components` path — the agent
//! assembly code is unchanged.
//!
//! See `docs/...` / `work_dirs/clawseed-persona-feature-plan.md` for the design.

use std::sync::Arc;

use clawseed_api::memory_traits::Memory;
use clawseed_config::schema::{AgentEntryConfig, Config, IdentityConfig};
use clawseed_memory::namespaced::NamespacedMemory;

/// The output of persona resolution: a `Config` clone with persona overrides
/// applied, and an optional `NamespacedMemory` wrapping the shared backend.
///
/// Callers should use `config` in place of the global config and
/// `memory.unwrap_or(shared_memory)` in place of the shared memory.
pub struct PersonaOverrides {
    /// Cloned config with identity/system_prompt/tool overrides applied.
    pub config: Config,
    /// `Some(NamespacedMemory)` when `memory_namespace` was set; `None` when
    /// the persona shares the global memory space.
    pub memory: Option<Arc<dyn Memory>>,
}

/// Resolve a named persona into config + memory overrides.
///
/// - `name` is `None` or not present in `config.agents` → returns `None`
///   (caller uses the global config + shared memory as-is).
/// - The entry exists but `has_persona_overrides()` is false (only an
///   `api_key`) → returns `None` (no persona behaviour to apply).
/// - Otherwise → clones the config, applies soul/tool overrides, and wraps
///   memory in `NamespacedMemory` if `memory_namespace` is set.
///
/// Soul override is mutually exclusive:
/// - If `entry.identity` is set → it replaces `config.identity`, and any
///   `config.agent.system_prompt` is cleared (identity wins).
/// - Else if `entry.system_prompt` is set → it replaces
///   `config.agent.system_prompt`, and `config.identity` is reset to the
///   default (openclaw, no AIEOS) so the prompt builder's system_prompt branch
///   takes effect. This guarantees a persona's system_prompt is not shadowed
///   by a global AIEOS identity.
/// - Neither set → `config.identity` and `config.agent.system_prompt` are
///   left untouched (persona only customises memory/tools, inheriting the
///   global soul).
pub fn resolve_persona(
    config: &Config,
    name: Option<&str>,
    shared_memory: Arc<dyn Memory>,
) -> Option<PersonaOverrides> {
    let name = name?;
    let entry = config.agents.get(name)?;
    if !entry.has_persona_overrides() {
        return None;
    }

    let mut cfg = config.clone();
    apply_soul_override(&mut cfg, entry);
    apply_tool_overrides(&mut cfg, entry);

    let memory = entry
        .memory_namespace
        .as_ref()
        .map(|ns| Arc::new(NamespacedMemory::new(shared_memory, ns.clone())) as Arc<dyn Memory>);

    Some(PersonaOverrides {
        config: cfg,
        memory,
    })
}

/// Apply the mutually-exclusive soul override (identity XOR system_prompt).
fn apply_soul_override(cfg: &mut Config, entry: &AgentEntryConfig) {
    if let Some(identity) = entry.identity.as_ref() {
        cfg.identity = identity.clone();
        // Identity wins over system_prompt — clear any prompt override so the
        // two don't compete in the prompt builder's priority table.
        cfg.agent.system_prompt = None;
    } else if let Some(system_prompt) = entry.system_prompt.as_ref() {
        cfg.agent.system_prompt = Some(system_prompt.clone());
        // Reset identity to default so a global AIEOS identity doesn't shadow
        // the persona's system_prompt (AIEOS has higher priority in the
        // prompt builder). With a default openclaw identity and no
        // personality_dir, the builder falls through to the system_prompt.
        cfg.identity = IdentityConfig::default();
    }
}

/// Apply tool-filter overrides (only when the persona sets them).
fn apply_tool_overrides(cfg: &mut Config, entry: &AgentEntryConfig) {
    if !entry.allowed_tools.is_empty() {
        cfg.agent.allowed_tools = entry.allowed_tools.clone();
    }
    if !entry.denied_tools.is_empty() {
        cfg.agent.denied_tools = entry.denied_tools.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity;
    use clawseed_config::schema::{AgentConfig, AgentEntryConfig, IdentityConfig};
    use clawseed_memory::none::NoneMemory;

    fn base_config() -> Config {
        Config {
            identity: IdentityConfig {
                format: "aieos".into(),
                aieos_inline: Some(r#"{"identity":{"names":{"first":"GlobalAieos"}}}"#.into()),
                aieos_path: None,
                personality_dir: None,
            },
            agent: AgentConfig {
                allowed_tools: vec!["global_*".into()],
                denied_tools: vec![],
                system_prompt: None,
                ..Default::default()
            },
            // Give the global memory namespace a marker so we can verify it's
            // left untouched by persona resolution.
            memory: clawseed_config::schema::MemoryConfig {
                namespace: Some("global_ns".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn none_mem() -> Arc<dyn Memory> {
        Arc::new(NoneMemory::new())
    }

    #[test]
    fn none_name_returns_none() {
        let cfg = base_config();
        assert!(resolve_persona(&cfg, None, none_mem()).is_none());
    }

    #[test]
    fn unknown_name_returns_none() {
        let cfg = base_config();
        assert!(resolve_persona(&cfg, Some("missing"), none_mem()).is_none());
    }

    #[test]
    fn api_key_only_entry_returns_none() {
        let mut cfg = base_config();
        cfg.agents.insert(
            "keyonly".into(),
            AgentEntryConfig {
                api_key: Some("sk-x".into()),
                ..Default::default()
            },
        );
        assert!(resolve_persona(&cfg, Some("keyonly"), none_mem()).is_none());
    }

    #[test]
    fn identity_override_replaces_global_identity_and_clears_system_prompt() {
        let mut cfg = base_config();
        cfg.agent.system_prompt = Some("global prompt".into());
        cfg.agents.insert(
            "nova".into(),
            AgentEntryConfig {
                identity: Some(IdentityConfig {
                    format: "aieos".into(),
                    aieos_inline: Some(r#"{"identity":{"names":{"first":"Nova"}}}"#.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("nova"), none_mem()).expect("persona resolved");
        assert_eq!(
            ov.config.identity.aieos_inline.as_deref(),
            Some(r#"{"identity":{"names":{"first":"Nova"}}}"#)
        );
        assert_eq!(ov.config.identity.format, "aieos");
        assert!(
            ov.config.agent.system_prompt.is_none(),
            "system_prompt must be cleared when identity is set"
        );
        assert!(
            ov.memory.is_none(),
            "no memory_namespace → no NamespacedMemory"
        );
        // Global config untouched.
        assert_eq!(
            cfg.identity.aieos_inline.as_deref(),
            Some(r#"{"identity":{"names":{"first":"GlobalAieos"}}}"#)
        );
    }

    #[test]
    fn system_prompt_override_resets_global_aieos_identity() {
        let mut cfg = base_config();
        cfg.agents.insert(
            "analyst".into(),
            AgentEntryConfig {
                system_prompt: Some("You are a terse analyst.".into()),
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("analyst"), none_mem()).expect("persona resolved");
        assert_eq!(
            ov.config.agent.system_prompt.as_deref(),
            Some("You are a terse analyst.")
        );
        // Identity must be reset to default (openclaw, no AIEOS) so the
        // system_prompt branch in the prompt builder actually fires.
        assert!(!identity::is_aieos_configured(&ov.config.identity));
        assert_eq!(ov.config.identity.format, "openclaw");
        assert!(ov.config.identity.aieos_inline.is_none());
        assert!(ov.config.identity.aieos_path.is_none());
        assert!(ov.config.identity.personality_dir.is_none());
    }

    #[test]
    fn memory_namespace_wraps_shared_memory() {
        let mut cfg = base_config();
        cfg.agents.insert(
            "iso".into(),
            AgentEntryConfig {
                memory_namespace: Some("persona_iso".into()),
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("iso"), none_mem()).expect("persona resolved");
        let mem = ov.memory.expect("NamespacedMemory should be present");
        assert_eq!(mem.name(), "none"); // wraps NoneMemory; name delegates to inner
    }

    #[test]
    fn tool_overrides_replace_global_filters() {
        let mut cfg = base_config();
        cfg.agents.insert(
            "locked".into(),
            AgentEntryConfig {
                allowed_tools: vec!["file_*".into(), "memory_*".into()],
                denied_tools: vec!["shell".into()],
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("locked"), none_mem()).expect("persona resolved");
        assert_eq!(ov.config.agent.allowed_tools, vec!["file_*", "memory_*"]);
        assert_eq!(ov.config.agent.denied_tools, vec!["shell"]);
        // Global untouched.
        assert_eq!(cfg.agent.allowed_tools, vec!["global_*"]);
    }

    #[test]
    fn no_soul_no_tools_only_namespace_still_resolves() {
        let mut cfg = base_config();
        cfg.agents.insert(
            "memonly".into(),
            AgentEntryConfig {
                memory_namespace: Some("persona_memonly".into()),
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("memonly"), none_mem()).expect("persona resolved");
        // Inherits global soul (AIEOS marker preserved).
        assert!(identity::is_aieos_configured(&ov.config.identity));
        assert!(ov.memory.is_some());
    }

    #[test]
    fn provider_and_memory_config_left_untouched() {
        let mut cfg = base_config();
        // base_config sets memory.namespace = "global_ns" as a marker.
        cfg.agents.insert(
            "nova".into(),
            AgentEntryConfig {
                identity: Some(IdentityConfig::default()),
                memory_namespace: Some("persona_nova".into()),
                ..Default::default()
            },
        );

        let ov = resolve_persona(&cfg, Some("nova"), none_mem()).expect("persona resolved");
        // cfg.memory.namespace (the column namespace) is NOT changed by persona.
        assert_eq!(ov.config.memory.namespace.as_deref(), Some("global_ns"));
    }
}
