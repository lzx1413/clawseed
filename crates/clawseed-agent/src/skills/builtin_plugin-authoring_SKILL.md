# ClawSeed Plugin Authoring

Use this workflow when the user asks to create or modify a local ClawSeed plugin.

## Workflow

1. Ask for or infer a stable lowercase `plugin_id`, the language (`python` or `node`), the plugin description, and the tools it should expose.
2. Call `plugin_create` to generate the plugin directory, `manifest.json`, entrypoint, and README. Keep the default permissions at `workspace=none` and `network=deny` unless the user explicitly needs broader access.
3. Ensure the active Linux runtime is installed and healthy. Plugin validation and execution always run inside that runtime.
4. Read the generated entrypoint and edit it with `file_edit` or `file_write`. Keep stdout reserved for one JSON response per request. Write diagnostics to stderr.
5. Implement CETP v2 methods `negotiate`, `get_provider_info`, `list_tools`, and `execute_tool`. Keep tool names lowercase with digits and underscores.
6. Call `plugin_validate` with the plugin ID. Fix manifest, protocol, schema, or startup errors before enabling the plugin.
7. Show the generated files, declared permissions, and validation result to the user. Ask for confirmation before changing `[plugins].enabled` or broadening permissions. Do not launch the plugin through host `exec`.

## Protocol requirements

- Read one JSON request per stdin line and write one JSON response per stdout line.
- Preserve the request `id` in every response.
- Return `status="success"` with a JSON `data` value for successful calls.
- Return `status="error"` with a structured `error` object for failures.
- The response envelope is `{"id":"...","status":"success","data":...}`. This is the CETP stdio binding, not generic JSON-RPC.
- `list_tools` returns `tools` with `name`, `description`, and `parameters`. Use `parameters`, not `inputSchema`.
- `execute_tool` receives `tool_name` and `args` inside `params`.
- `negotiate` returns `selected_version: 2` and `session_token`.
- Do not print logs, progress text, or stack traces to stdout.
- Keep `list_tools` schemas accurate because the Agent uses them for model tool selection.

## Permission rules

Use the smallest permissions that satisfy the request. Explain any request for workspace writes or network access. Plugin creation writes files into `.clawseed/plugins/<plugin_id>` and does not enable or execute the plugin by itself.
