# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest on `main` | Yes |

## Reporting a Vulnerability

If you discover a security vulnerability in ClawSeed, please report it responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, please email: **security@clawseed.dev**

Include:

- A description of the vulnerability
- Steps to reproduce
- Affected components (crate name, file, function)
- Impact assessment (what an attacker could achieve)
- Any suggested fix, if you have one

## Response Timeline

- **Acknowledgment**: within 48 hours
- **Initial assessment**: within 7 days
- **Fix and disclosure**: coordinated with the reporter, typically within 30 days

## Scope

The following are in scope:

- **clawseed-agent**: tool dispatch, security policy bypass, hook pipeline
- **clawseed-gateway**: HTTP/WebSocket endpoints, authentication, session isolation
- **clawseed-tools**: shell command injection, path traversal, file access controls
- **clawseed-providers**: credential handling, API key exposure
- **clawseed-config**: config parsing, environment variable expansion, secret masking
- **clawseed-memory**: SQL injection, data leakage between sessions
- **Android client**: local data storage, network security, intent handling

The following are out of scope:

- Vulnerabilities in upstream dependencies (report to the upstream project)
- Attacks requiring physical access to the device
- Social engineering

## Security Architecture

ClawSeed includes built-in security mechanisms:

- **Autonomy levels** (`ReadOnly` / `Supervised` / `Full`) control what the agent can do
- **SecurityPolicy** intercepts all tool calls via the `Hook` trait before execution
- **Command allowlists** validate shell commands against `allowed_commands`
- **Path guards** block access to sensitive paths (`/etc/passwd`, `/etc/shadow`, `~/.ssh/`)
- **Rate limiting** caps actions per session via `max_actions_per_hour`
- **API key masking** prevents credentials from being exposed via config APIs

## Disclosure Policy

We follow coordinated disclosure. We will credit reporters in the release notes unless they prefer to remain anonymous.
