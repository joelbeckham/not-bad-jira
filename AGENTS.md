## Native UI visual review

For any task that creates or materially changes GPUI user interfaces:

1. Implement the change in Rust using Codex.
2. Build and run the native Windows application.
3. Capture a screenshot of the changed interface and save it under `design/reviews/`.
4. Use T3 Code's `delegate_task` MCP tool to invoke a Claude agent:
   - Target provider: `claudeAgent`
   - Role: `review`
   - Mode: `wait`
   - Task: Critique the screenshot as a professional desktop UI designer.
5. Give Claude the screenshot path, relevant design references, and a brief description of the intended behavior.
6. Ask for concrete, prioritized improvements to spacing, alignment, typography, color, hierarchy, and visual consistency.
7. Implement the substantive improvements with Codex.
8. Capture a new screenshot and perform one more review if necessary.

Claude should act as a read-only design critic. It should not modify application code.

## UI copy: no filler

Apply this rule to all future UI changes, on every screen:

- Keep copy concise and functional: headings, field labels, action names, status,
  validation errors and necessary recovery steps.
- Do not add slogans, marketing copy, welcome blurbs, value propositions,
  redundant instructions or decorative helper text.
- Do not add implementation details or reassurance about storage, security or
  architecture unless explicitly requested or essential to completing an action.
- Show helper text only when it is needed to perform the action or prevent data
  loss, and only while that condition applies.
- Keep demo/test entry points out of the production UI. Use explicit CLI flags
  for demo and test modes.
- Prefer removing unnecessary copy to replacing it with different filler.
- Do not repeat field labels in placeholders or explain standard OS navigation.

## Application

Rust 2024, Windows MSVC. GPUI Kit is pinned to 0.9.1 (GPUI 0.3.12);
zorite-editor 0.11.3 was compiled and rendered with Kit on Windows.
Inter and JetBrains Mono are embedded; their OFL licenses are in assets/fonts.
The executable icon is generated specifically for this app.

Commands (PowerShell, repository root):

```powershell
cargo run -- --demo
cargo run -- --editor-check
cargo test --offline
cargo build --release --offline
./target/release/not-bad-jira.exe
./target/release/not-bad-jira.exe --demo --measure-startup
./scripts/capture-window.ps1 -Output design/reviews/native.png
```

`--demo` uses only sample data and never authenticates. `--editor-check` opens
the native editor compatibility screen. The optional startup measurement writes
`design/reviews/startup.txt`: Rust main to the second frame callback, an upper
bound on the first frame, excluding process-loader time. It is not a measured
production cached-account startup time. Record actual timings after release runs.

`src/app.rs` owns the UI, navigation stack and command dispatch. `src/actions.rs`
is shared by key bindings, palette and context menus. `src/jira.rs`, `src/adf.rs`
and `src/config.rs` have no GPUI dependency. Network calls use worker threads;
generation-tagged responses prevent old account requests from updating a new
session. Search/detail responses also carry an edit revision, so requests begun
before an edit cannot replace its result. List refreshes merge fields and retain
cached descriptions/comments. Config changes are read on a worker; reload waits for active edits and
saves. Cache writes go through a serial worker and atomic file replacement.

Config is `%APPDATA%/not-bad-jira/config.yaml`, cache is
`%LOCALAPPDATA%/not-bad-jira/cache/issues.json` (8 MiB cap, versioned and bound
to account and tab queries). Tokens belong only in Windows Credential Manager,
under service `not-bad-jira`. Never log tokens or authorization headers. Legacy
settings were copied once, outside the app. Do not add automatic legacy
config discovery, import/migration code, compatibility paths or migration UI.
The app reads only its own config and credentials. Demo mode bypasses real
config, cache and credentials.

ADF changes need conversion tests: preserve unknown nodes as protected tokens,
reject missing/duplicated tokens, keep failed drafts, and never replace an
unrecognized structure with empty text. Existing blocks with unsupported layout
attributes stay protected. Creating issues can fail when a Jira project requires
additional custom fields; retain the summary and show Jira's validation error.

Native screenshots live under design/reviews. Capture is DPI-aware; Windows'
invisible resize border may appear around a window. Prefer accessibility actions
for native smoke tests. Keyboard smoke tests post only to the test app's exact
window handle. Never send test input to other apps.

Validation on 2026-11-13: 15 tests pass; Clippy passes with `-D warnings`.
Native smoke checks cover list filtering, J/Enter detail navigation, command
filtering, issue-key search + Enter, summary editor and setup.
Claude's final visual recheck reports the UI ready for user testing. Authenticated
Jira operations still need validation against an account/project; local HTTP
fixtures cover search token pagination, 204 updates and authentication errors.

Release measurement (2026-11-13): 19,018,752 bytes (18.1 MiB). The release
demo reports 423.88 ms from Rust main to the second-frame callback. This upper
bound is not proof of the 150 ms cached-account startup target; actual first
presentation, production-cache startup and input-latency profiling remain open.
The release executable was launched successfully and its window was responsive.
See design/reviews/release-final.png and design/reviews/startup.txt.
