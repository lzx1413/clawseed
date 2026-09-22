use clawseed_api::provider::{ResponseMetrics, TokenUsage};
use clawseed_api::tool::ToolSpec;
use std::time::Duration;

/// A rough provider prompt size calibrated against the provider's last exact
/// input-token report. The raw estimate is used only for measuring changes in
/// the request; the previous provider value supplies the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PromptCalibration {
    pub(super) raw_tokens: usize,
    pub(super) actual_input_tokens: u64,
}

impl PromptCalibration {
    pub(super) fn estimate(&self, raw_tokens: usize) -> usize {
        let tokens = if raw_tokens >= self.raw_tokens {
            self.actual_input_tokens
                .saturating_add((raw_tokens - self.raw_tokens) as u64)
        } else {
            self.actual_input_tokens
                .saturating_sub((self.raw_tokens - raw_tokens) as u64)
        };
        tokens.min(usize::MAX as u64) as usize
    }
}

/// Estimate the full prompt sent to a provider, including tool definitions.
/// The message/tool split is retained for the debug UI, while `total_tokens`
/// is the value that should be compared with a context window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PromptEstimate {
    pub(super) total_tokens: usize,
    pub(super) tool_tokens: usize,
}

pub(super) fn estimate_prompt_tokens(
    messages: &[clawseed_api::provider::ChatMessage],
    tool_specs: &[ToolSpec],
    include_tools: bool,
) -> PromptEstimate {
    let tool_tokens = if include_tools {
        serde_json::to_string(tool_specs)
            .map(|tools| tools.len().div_ceil(4))
            .unwrap_or(0)
    } else {
        0
    };
    PromptEstimate {
        total_tokens: crate::history::estimate_history_tokens(messages).saturating_add(tool_tokens),
        tool_tokens,
    }
}

/// Collect metrics for a streamed assistant turn.
///
/// Input and cached-input counts describe the latest provider request. Output
/// and generation time remain summed across tool iterations because they
/// describe the complete assistant response.
pub(super) struct TurnMetrics {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    generation_seconds: Option<f64>,
}

impl super::Agent {
    pub(super) fn calibrated_prompt_tokens(&self, raw_tokens: usize) -> usize {
        self.prompt_calibration
            .map(|calibration| calibration.estimate(raw_tokens))
            .unwrap_or(raw_tokens)
    }

    pub(super) fn remember_prompt_calibration(
        &mut self,
        raw_tokens: usize,
        usage: Option<&TokenUsage>,
    ) {
        if let Some(actual_input_tokens) = usage.and_then(|usage| usage.input_tokens) {
            self.prompt_calibration = Some(PromptCalibration {
                raw_tokens,
                actual_input_tokens,
            });
        }
    }

    pub(super) fn clear_prompt_calibration(&mut self) {
        self.prompt_calibration = None;
    }
}

impl Default for TurnMetrics {
    fn default() -> Self {
        Self {
            input: Some(0),
            output: Some(0),
            cached: Some(0),
            generation_seconds: Some(0.0),
        }
    }
}

impl TurnMetrics {
    pub(super) fn record(&mut self, usage: Option<&TokenUsage>, generation: Option<Duration>) {
        self.input = usage.and_then(|u| u.input_tokens);
        self.output = self
            .output
            .zip(usage.and_then(|u| u.output_tokens))
            .and_then(|(a, b)| a.checked_add(b));
        self.cached = usage.and_then(|u| u.cached_input_tokens);
        self.generation_seconds = self
            .generation_seconds
            .zip(generation)
            .map(|(a, b)| a + b.as_secs_f64());
    }

    pub(super) fn finish(self, elapsed: Duration) -> ResponseMetrics {
        ResponseMetrics {
            input_tokens: self.input,
            output_tokens: self.output,
            cached_input_tokens: self.cached,
            cache_hit_ratio: self
                .cached
                .zip(self.input)
                .filter(|(cached, input)| *input > 0 && cached <= input)
                .map(|(cached, input)| cached as f64 / input as f64),
            output_tokens_per_second: self
                .output
                .zip(self.generation_seconds)
                .filter(|(_, seconds)| *seconds > 0.0)
                .map(|(output, seconds)| output as f64 / seconds),
            elapsed_ms: elapsed.as_millis().try_into().unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_tool_iterations_by_tokens_and_excludes_tool_wait() {
        let mut metrics = TurnMetrics::default();
        metrics.record(
            Some(&TokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(10),
                cached_input_tokens: Some(50),
            }),
            Some(Duration::from_secs(1)),
        );
        metrics.record(
            Some(&TokenUsage {
                input_tokens: Some(900),
                output_tokens: Some(30),
                cached_input_tokens: Some(900),
            }),
            Some(Duration::from_secs(1)),
        );
        let result = metrics.finish(Duration::from_secs(10));
        assert_eq!(result.input_tokens, Some(900));
        assert_eq!(result.output_tokens, Some(40));
        assert_eq!(result.cache_hit_ratio, Some(1.0));
        assert_eq!(result.output_tokens_per_second, Some(20.0));
        assert_eq!(result.elapsed_ms, 10000);
    }

    #[test]
    fn latest_usage_replaces_an_unknown_previous_request() {
        let mut metrics = TurnMetrics::default();
        metrics.record(None, None);
        metrics.record(
            Some(&TokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(10),
                cached_input_tokens: Some(0),
            }),
            Some(Duration::from_secs(1)),
        );
        let result = metrics.finish(Duration::from_secs(2));
        assert_eq!(result.input_tokens, Some(100));
        assert_eq!(result.output_tokens, None);
        assert_eq!(result.cache_hit_ratio, Some(0.0));
        assert_eq!(result.output_tokens_per_second, None);
    }

    #[test]
    fn zero_input_has_no_ratio_and_zero_cache_is_known() {
        let mut metrics = TurnMetrics::default();
        metrics.record(
            Some(&TokenUsage {
                input_tokens: Some(0),
                output_tokens: Some(0),
                cached_input_tokens: Some(0),
            }),
            None,
        );
        let result = metrics.finish(Duration::ZERO);
        assert_eq!(result.cached_input_tokens, Some(0));
        assert_eq!(result.cache_hit_ratio, None);
    }

    #[test]
    fn calibrated_prompt_estimate_uses_the_last_exact_input_as_baseline() {
        let calibration = PromptCalibration {
            raw_tokens: 9_312,
            actual_input_tokens: 12_696,
        };
        assert_eq!(calibration.estimate(9_312), 12_696);
        assert_eq!(calibration.estimate(9_912), 13_296);
        assert_eq!(calibration.estimate(8_912), 12_296);
    }

    #[test]
    fn prompt_estimate_includes_tools_in_total() {
        let messages = vec![clawseed_api::provider::ChatMessage::user("hello")];
        let tools = vec![ToolSpec {
            name: "lookup".into(),
            description: "look up a value".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let estimate = estimate_prompt_tokens(&messages, &tools, true);
        assert!(estimate.total_tokens > estimate.tool_tokens);
        assert_eq!(
            estimate.total_tokens,
            crate::history::estimate_history_tokens(&messages) + estimate.tool_tokens
        );
    }
}
