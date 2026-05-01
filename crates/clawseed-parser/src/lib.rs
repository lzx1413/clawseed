//! LLM tool call parser for ClawSeed.
//!
//! Parses tool calls from LLM responses in multiple formats:
//! - OpenAI native JSON `tool_calls` array
//! - XML tags: ◁, <toolcall>, <invoke>
//! - Markdown code blocks: ```tool_call
//! - Anthropic <FunctionCall> tags
//! - GLM/MiniMax/Perl style

pub mod parser;
