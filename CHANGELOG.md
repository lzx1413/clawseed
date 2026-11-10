# Changelog

## 2.2.0 — 2026-11-10

### Android

- Refreshed the interface with shared glass surfaces, updated chat controls, and consistent navigation and settings components.
- Added persona-specific new-chat shortcuts with customizable prompts, ordering, icons, and visibility.
- Added long-press selection and confirmed batch deletion for conversation history.
- Added MiMo cloud voice playback and persistent turn activity presentation.
- Improved persona chat creation, data management navigation, profile deletion confirmations, keyboard insets, and foreground-service notification navigation.
- Fixed an update-install crash and validate cached APKs against the selected release before reuse or installation.

### Runtime and tools

- Added a CSV processing tool with validation and safer file writes.
- Added gateway support for MiMo text-to-speech and persisted session activity metadata.

### Verification

- Local CI and Android unit tests pass; release APK builds successfully.
- The Android app was tested on a physical device.

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
