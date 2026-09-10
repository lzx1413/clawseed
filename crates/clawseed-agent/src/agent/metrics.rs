use clawseed_api::provider::{ResponseMetrics, TokenUsage};
use std::time::Duration;

/// Sum complete per-request counts, preserving unknowns across tool iterations.
pub(super) struct TurnMetrics {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    generation_seconds: Option<f64>,
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
        self.input = self
            .input
            .zip(usage.and_then(|u| u.input_tokens))
            .and_then(|(a, b)| a.checked_add(b));
        self.output = self
            .output
            .zip(usage.and_then(|u| u.output_tokens))
            .and_then(|(a, b)| a.checked_add(b));
        self.cached = self
            .cached
            .zip(usage.and_then(|u| u.cached_input_tokens))
            .and_then(|(a, b)| a.checked_add(b));
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
        assert_eq!(result.input_tokens, Some(1000));
        assert_eq!(result.output_tokens, Some(40));
        assert_eq!(result.cache_hit_ratio, Some(0.95));
        assert_eq!(result.output_tokens_per_second, Some(20.0));
        assert_eq!(result.elapsed_ms, 10000);
    }

    #[test]
    fn missing_usage_is_not_zero_or_a_partial_total() {
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
        assert_eq!(result.input_tokens, None);
        assert_eq!(result.output_tokens, None);
        assert_eq!(result.cache_hit_ratio, None);
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
}
