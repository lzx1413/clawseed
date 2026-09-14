//! Hook resolution and tool execution for agent turns.

use super::Agent;
use crate::context::AgentToolContext;
use crate::dispatcher::{ParsedToolCall, ToolExecutionResult};
use crate::observer::ObserverEvent;
use clawseed_api::tool::ToolResult;
use std::time::Instant;

fn validated_presentation(
    tool_name: &str,
    presentation: Option<clawseed_api::tool::ToolPresentation>,
) -> Option<clawseed_api::tool::ToolPresentation> {
    presentation.and_then(|presentation| match presentation.validate() {
        Ok(()) => Some(presentation),
        Err(error) => {
            tracing::warn!(tool = tool_name, %error, "discarding invalid tool presentation");
            None
        }
    })
}

/// A tool call resolved through before-hooks, ready for execution.
struct ResolvedToolCall {
    name: String,
    args: serde_json::Value,
    /// Set when a before-hook cancelled this call; `output` contains the reason.
    cancelled: bool,
    output: String,
    tool_call_id: Option<String>,
}

impl Agent {
    /// Build the tool context for a single tool execution.
    fn build_tool_context(&self, tool_call_id: Option<String>) -> AgentToolContext {
        AgentToolContext::new(
            self.workspace_dir.clone(),
            self.user_context.clone(),
            self.active_turn_id.clone(),
            tool_call_id,
        )
    }

    async fn execute_tool_call(&self, call: &ParsedToolCall) -> ToolExecutionResult {
        let start = Instant::now();

        // Hook: before_tool_call
        let mut tool_name = call.name.clone();
        let mut tool_args = call.arguments.clone();
        if let Some(ref hooks) = self.hook_runner {
            match hooks
                .run_before_tool_call(tool_name.clone(), tool_args.clone())
                .await
            {
                crate::hooks::HookRunnerResult::Continue { name, arguments } => {
                    tool_name = name;
                    tool_args = arguments;
                }
                crate::hooks::HookRunnerResult::Cancel(reason) => {
                    tracing::info!(tool = %call.name, %reason, "tool call cancelled by hook");
                    return ToolExecutionResult {
                        name: call.name.clone(),
                        output: format!("Cancelled by hook: {reason}"),
                        success: false,
                        tool_call_id: call.tool_call_id.clone(),
                        presentation: None,
                    };
                }
            }
        }

        // Execute the tool
        let ctx = self.build_tool_context(call.tool_call_id.clone());
        let (result, success, presentation) =
            if let Some(tool) = self.tool_registry.get_tool(&tool_name) {
                match tool.execute(tool_args.clone(), &ctx).await {
                    Ok(r) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: tool_name.clone(),
                            duration: start.elapsed(),
                            success: r.success,
                        });
                        let presentation = validated_presentation(&tool_name, r.presentation);
                        if r.success {
                            (r.output, true, presentation)
                        } else {
                            (
                                format!("Error: {}", r.error.unwrap_or(r.output)),
                                false,
                                presentation,
                            )
                        }
                    }
                    Err(e) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: tool_name.clone(),
                            duration: start.elapsed(),
                            success: false,
                        });
                        (format!("Error executing {}: {e}", tool_name), false, None)
                    }
                }
            } else {
                (format!("Unknown tool: {}", tool_name), false, None)
            };

        let duration = start.elapsed();

        // Hook: after_tool_call
        if let Some(ref hooks) = self.hook_runner {
            let tool_result_obj = ToolResult {
                success,
                output: result.clone(),
                error: None,
                presentation: presentation.clone(),
            };
            hooks
                .fire_after_tool_call(&tool_name, &tool_result_obj, duration)
                .await;
        }

        ToolExecutionResult {
            name: tool_name,
            output: result,
            success,
            tool_call_id: call.tool_call_id.clone(),
            presentation,
        }
    }

    pub(super) async fn execute_tools(&self, calls: &[ParsedToolCall]) -> Vec<ToolExecutionResult> {
        if calls.len() <= 1 {
            let mut results = Vec::with_capacity(calls.len());
            for call in calls {
                results.push(self.execute_tool_call(call).await);
            }
            return results;
        }

        // Multiple tool calls: run before-hooks serially, then execute in parallel
        let resolved = self.resolve_before_hooks(calls).await;
        let futures: Vec<_> = resolved
            .iter()
            .map(|r| self.execute_resolved_tool(r))
            .collect();
        futures_util::future::join_all(futures).await
    }

    /// Resolve before-hooks for all tool calls (serially), returning the
    /// resolved names/args and whether each was cancelled.
    async fn resolve_before_hooks(&self, calls: &[ParsedToolCall]) -> Vec<ResolvedToolCall> {
        let mut resolved = Vec::with_capacity(calls.len());
        for call in calls {
            let mut tool_name = call.name.clone();
            let mut tool_args = call.arguments.clone();

            if let Some(ref hooks) = self.hook_runner {
                match hooks
                    .run_before_tool_call(tool_name.clone(), tool_args.clone())
                    .await
                {
                    crate::hooks::HookRunnerResult::Continue { name, arguments } => {
                        tool_name = name;
                        tool_args = arguments;
                    }
                    crate::hooks::HookRunnerResult::Cancel(reason) => {
                        tracing::info!(tool = %call.name, %reason, "tool call cancelled by hook");
                        resolved.push(ResolvedToolCall {
                            name: call.name.clone(),
                            args: serde_json::Value::Null,
                            cancelled: true,
                            output: format!("Cancelled by hook: {reason}"),
                            tool_call_id: call.tool_call_id.clone(),
                        });
                        continue;
                    }
                }
            }

            resolved.push(ResolvedToolCall {
                name: tool_name,
                args: tool_args,
                cancelled: false,
                output: String::new(),
                tool_call_id: call.tool_call_id.clone(),
            });
        }
        resolved
    }

    /// Execute a single resolved tool call (no before-hooks, they already ran).
    async fn execute_resolved_tool(&self, resolved: &ResolvedToolCall) -> ToolExecutionResult {
        if resolved.cancelled {
            return ToolExecutionResult {
                name: resolved.name.clone(),
                output: resolved.output.clone(),
                success: false,
                tool_call_id: resolved.tool_call_id.clone(),
                presentation: None,
            };
        }

        let start = Instant::now();
        let ctx = self.build_tool_context(resolved.tool_call_id.clone());
        let (result, success, presentation) =
            if let Some(tool) = self.tool_registry.get_tool(&resolved.name) {
                match tool.execute(resolved.args.clone(), &ctx).await {
                    Ok(r) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: resolved.name.clone(),
                            duration: start.elapsed(),
                            success: r.success,
                        });
                        let presentation = validated_presentation(&resolved.name, r.presentation);
                        if r.success {
                            (r.output, true, presentation)
                        } else {
                            (
                                format!("Error: {}", r.error.unwrap_or(r.output)),
                                false,
                                presentation,
                            )
                        }
                    }
                    Err(e) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: resolved.name.clone(),
                            duration: start.elapsed(),
                            success: false,
                        });
                        (
                            format!("Error executing {}: {e}", resolved.name),
                            false,
                            None,
                        )
                    }
                }
            } else {
                (format!("Unknown tool: {}", resolved.name), false, None)
            };

        let duration = start.elapsed();

        // Hook: after_tool_call
        if let Some(ref hooks) = self.hook_runner {
            let tool_result_obj = ToolResult {
                success,
                output: result.clone(),
                error: None,
                presentation: presentation.clone(),
            };
            hooks
                .fire_after_tool_call(&resolved.name, &tool_result_obj, duration)
                .await;
        }

        ToolExecutionResult {
            name: resolved.name.clone(),
            output: result,
            success,
            tool_call_id: resolved.tool_call_id.clone(),
            presentation,
        }
    }
}
