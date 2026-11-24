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
- The host puts current-turn images in both `params.attachments` and `params.context.attachments`. Each item has a runtime-visible `path`, `id`, `mime_type`, `size_bytes`, `width`, and `height`. Read the path during the call and do not retain it after the response.
- `execute_tool` may return `data.output` for LLM-facing text and an optional `data.presentation` object with versioned `blocks` such as `image`, `audio`, `video`, or `link`. The Android client renders these blocks.
- `negotiate` returns `selected_version: 2` and `session_token`.
- Do not print logs, progress text, or stack traces to stdout.
- Keep `list_tools` schemas accurate because the Agent uses them for model tool selection.

Media presentation example:

~~~json
{"id":"req-1","status":"success","data":{"output":"天气图","presentation":{"version":1,"blocks":[{"type":"image","media":{"url":"https://cdn.example.com/weather.png","mime_type":"image/png"},"alt":"天气图"}]}}}
~~~

Media URLs must use HTTPS. The host validates presentation blocks before they reach the client.

For plugin-generated local media, return a workspace-relative `media.path` with the MIME type.
The Android client reads that path from its local workspace and renders the file. Do not generate
opaque asset IDs or return host absolute paths. The client-provided `media_present` tool is for
explicit display requests and is not needed when the plugin result already contains a
`presentation` block.

## Permission rules

Use the smallest permissions that satisfy the request. The generated default is
`workspace=none` and `network=deny`.

- `workspace=none` does not mount `/workspace`. The plugin cannot access the workspace.
- `workspace=read` exposes a temporary snapshot at `/workspace`. The plugin can read it, but changes are discarded when the process exits and the real workspace is not modified.
- `workspace=write` mounts the real workspace at `/workspace`. The plugin can create, modify, and delete workspace files.
- The plugin process starts in `/plugin`. Relative paths such as `data/output.png` point into the plugin code mount. App-visible media must be written below `/workspace`, then returned as a path relative to the workspace root, for example `.clawseed/plugins/weather/media/output.png`.
- `network=deny` and `network=allow` describe the plugin's network requirement. They do not provide kernel-level network isolation.
- `environment` is a whitelist of host environment variable names passed to the plugin.

Explain any request for workspace writes or network access. Plugin creation writes files into `.clawseed/plugins/<plugin_id>` and does not enable or execute the plugin by itself.
