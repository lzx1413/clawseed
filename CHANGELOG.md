# Changelog

## 2.1.0 — 2026-10-22

### Android

- Added local and WebDAV backup destinations, scheduled automatic backups, retention cleanup, connection diagnostics, and historical restore selection.
- Kept active conversations running while the app is in the background, with progress persistence, notifications, cancellation, timeout cleanup, and foreground-service execution.
- Added voice engine settings, interactive dashboard skills, chart rendering, and improved persona-based session organization.
- Added local backup folder permissions and persisted automatic-backup configuration.

### Runtime and tools

- Added DeepSeek web search provider routing.
- Added token-aware context compaction and surfaced compaction progress in the Android client.
- Added structured user interactions, managed background commands, and a sandboxed QuickJS evaluator.
- Improved web fetch extraction, memory updates, provider/persona routing, CETP session handling, and streamed ask-user events.
- Bumped the Rust workspace and embedded SDK version to 0.5.0.

### Verification

- Rust formatting, Clippy, feature checks, CI build, and 1,506 workspace tests pass.
- Android native gateway, Debug unit tests, and Release APK build pass.

## 2.0.0 — 2026-09-11

- Added image and file conversations for Android, including PDF, DOCX, Markdown, text, and CSV attachments.
