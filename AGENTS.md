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

Waiting status icon (2026-11-16): exact `Waiting` names (case-insensitive,
trimmed) use an amber clock, distinct from In Progress's blue circle-dot.
Other statuses retain category-based fallbacks. The shared renderer applies to
lists, details, related rows and pickers. Use `--demo --demo-statuses` for the
comparison fixture. Release native list/detail captures are
`design/reviews/waiting-status-list.png` and `waiting-status-detail.png`.
Claude reviewed both and recommended no substantive changes. All 51 tests,
Clippy with `-D warnings`, formatting and the release build pass.

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
- Keep shortcut hints in the footer; do not repeat them inside action-button
  labels when the footer already shows the same binding.

## Application

Rust 2024, Windows MSVC. GPUI Kit is pinned to 0.9.1 (GPUI 0.3.12);
zorite-editor 0.11.3 was compiled and rendered with Kit on Windows.
Inter and JetBrains Mono are embedded; their OFL licenses are in assets/fonts.
The title-bar and executable icons use the user-supplied Jira artwork in
`assets/jira.webp`. Run `cargo run --example prepare_icon --offline` to regenerate
the embedded PNG and multi-resolution Windows ICO. Keep the title bar to the
icon and app name, without a workspace/account suffix.

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

`src/picker.rs` owns filtering and selection without GPUI dependencies. Field
popups keep the filter focused; navigation keys are captured before input editing
actions. Request IDs discard responses for closed or replaced pickers. Keep the
transition action ID separate from the target status ID, and preserve target
status categories for icons. Demo field selections change sample data only in
memory.

In the main issue filter, Enter or Down hands focus to the issue list without
opening an issue or skipping the selected row. Typing a new query resets list
selection to the first match. Clicking the filter text or pressing `/` focuses
the input with its query intact; its mouse events must not reach the workspace
handler that focuses the list. Property popup and related filters keep their
own typing/navigation behavior.

In the main issue list, vertical wheel motion moves the selection at most one
row per event, even when Windows reports multiple lines per notch. Accumulate
fractional motion without queuing excess whole rows. Keep the viewport
still while the selection is visible; reveal it with `ScrollStrategy::Nearest`
only at an edge. Keyboard row navigation uses the same reveal behavior. Ignore
horizontal wheel motion for row selection. Do not steal focus from the filter
or change popup/detail scrolling. `--demo --demo-list` supplies 80 sample rows
for native checks; `scripts/post-test-wheel.ps1` targets only the given app PID.

Issue context menus contain only actions for the clicked issue. Keep navigation,
refresh, creation, account/config and quit commands in the general palette.
Show shortcuts in the existing `keycap` style, lowercase and aligned to the
right without parentheses. Group editing, sharing and deletion with separators.
Capture the issue key and resolve its current row when an action runs; a refreshed or
reordered list must never redirect a menu action to another issue. Custom menu
labels use accessible text values so their parent menu items retain names.

Brief action notices use the existing bottom-left footer status area for copying keys
or URLs, launching browser/config actions, sorting, and successful mutations.
Keep the accompanying icon and concise, non-focusable text; do not add a floating
card or a second status area. The latest notice replaces the previous one and
expires after 3.5 seconds, restoring the underlying status. New errors, progress
and editing actions dismiss the notice so they are visible immediately.
Only confirm Jira writes after a successful response; opening an external app
is a request, so use "Opening", without claiming it finished. Clear notices
when changing accounts. Avoid notices for ordinary navigation or typing.

List `created`, `updated` and `duedate` cells show short calendar dates such as
`Oct 27, 2026`, using Jira's date without timezone conversion. `Issue::list_field`
formats display values; raw timestamps remain intact for sorting and caching.
Fixed-width text columns stay on one line with ellipsis when necessary.
Use `--demo --demo-dates` for a native date-column preview.

`src/avatar.rs` loads assignee avatars from Jira's `avatarUrls`. Rendering only
uses memory and a bounded queue; a separate disk worker keeps cache hits clear
of the two HTTP workers. Cached images work before account authentication and
offline, refresh after 24 hours, and remain visible if refreshing fails. Memory
holds up to 256 avatars; `cache/avatars` holds up to 512 PNGs / 16 MiB, keyed by
account and URL. Requests are deduplicated, size-limited, and retried after five
minutes on failure. Avatar redirects must never forward Jira credentials to
another origin. Demo mode shows placeholders without fetching or reading caches.

Issue type icons use Jira's `issuetype.iconUrl` through the same image cache,
including SVGs rasterized on a worker with external image references disabled.
`src/issue_type.rs` supplies themed fallbacks. Show the icon beside issue keys;
an explicit type column takes its place in list tabs that already include one.

`src/related.rs` groups parent, direct children, subtasks and directional issue
links. Preserve Jira's inward/outward labels, including custom link types.
Related rows reuse the list's type, status, priority and assignee indicators;
keep keys readable and summaries on one line. Groups use virtual lists with a
visible scrollbar when they overflow. Click opens a detail; Tab focuses a group,
arrows/J/K/Home/End navigate and Enter opens. On a detail, Enter or `/` starts
inline related filtering. Keep the filter focused while Up/Down move through
visible rows across groups; J/K remain text input. Enter opens the selection.
Esc cancels filtering and restores the detail's prior scroll; another Esc goes
back. Size key columns from all related rows so filtering does not shift them.
Related metadata loads on a worker, paginates and is cached without replacing
descriptions/comments. Keep account, request and edit-revision response guards.

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
Keep the creation editor visible through issue-type selection and the create
request. Label its primary action and footer hint `Create`, with `Creating…`
while pending. Show the spinner/status, disable summary/submit/cancel,
and consume duplicate submission/cancel keys. Only navigate to the new issue
after the create worker returns its details. Preserve the draft and inline error
on rejection. A failed detail fetch after successful POST must open the confirmed
issue with a load-only Retry, never repeat the create POST. Creation responses
carry an account epoch and request ID. Demo creation is memory-only; use
`--demo-create-delay=12000` and `--demo-create-error` with `--demo` for native
pending/failure checks (the error flag fails only the first submission).

Native screenshots live under design/reviews. Capture is DPI-aware; Windows'
invisible resize border may appear around a window. Prefer accessibility actions
for native smoke tests. Keyboard smoke tests post only to the test app's exact
window handle. Never send test input to other apps.
Pass `-TargetProcessId` to native smoke and capture scripts so a closed preview
cannot redirect a check to another app window.
Before replacing a running app, preserve open editors/type pickers and pending
`Saving…` or `Creating…` requests; never discard a user's draft for a rebuild.
An in-use build may be retained as `target/release/not-bad-jira-running-<pid>.exe`
to keep its draft window open during an update. Keep that file until the process
exits; Windows may still report the process's original executable path.

Validation on 2026-11-14: 46 tests pass; Clippy passes with `-D warnings`.
Native smoke checks cover list filtering, J/Enter detail navigation, command
filtering, issue-key search + Enter, summary editor and setup.
Main-filter checks also cover Enter/Down focus handoff without opening/skipping,
second Enter opening the selected row, slash refocusing with query preserved,
actual mouse clicks on filter text restoring typing and safe empty results.
Date formatting checks cover calendar-day preservation across timezone offsets,
date-only due dates, leap days and missing/malformed values. The native date
column was checked in `design/reviews/list-dates.png`; Claude review was skipped
for this change at the user's request.
Action-notice checks cover clipboard key/URL contents, retained keyboard focus,
latest-message replacement/expiry, sort changes, field-update confirmation and
empty selections. Clipboard tests restore the prior clipboard contents. Native
screenshots are in `design/reviews/action-notice-*.png`; Claude remains skipped
while offline at the user's request.
Footer notice placement, return to the underlying status, sorting feedback and
immediate creation validation were checked natively in
`design/reviews/footer-notice-*.png`. Notices share the existing footer rather
than overlaying the content; the confirmation icon is retained.
Wheel checks cover selection within the viewport, edge reveal, reversal,
horizontal input, both list boundaries, Enter drill-in, filtered and empty
results, plus subsequent keyboard navigation. Multi-line wheel input advances
exactly one row in either direction, with no queued whole-row movement.
Screenshots are in
`design/reviews/wheel-*.png`; Claude remains skipped while offline at the user's
request. Unit tests cover fractional deltas, direction changes, idle reset and
empty/shortened lists.
Issue context menus were checked for issue-only contents, right-click selection
of an unselected row, mouse priority changes on that issue, keyboard opening and
Escape dismissal. See `design/reviews/issue-context-menu.png`. Claude remains
skipped while offline at the user's request.
Field picker checks cover live typing, retained filter focus, Up/Down navigation,
Enter selection, mouse selection, Escape, no matches, current-value preservation
and scrolling to a current value beyond the first eight options.
Related issue checks cover directional/custom labels, parent/subtask/epic
grouping, mouse and keyboard drill-in, End scrolling in large groups and Esc
return navigation. Read-only authenticated checks loaded two custom-direction
links and five direct children of an epic successfully.
Inline navigation checks cover Enter activation, live text filtering, retained
input focus after arrows, selection across groups, safe empty results, scrolling
large groups and separate Esc cancel/back behavior.
Creation checks cover keeping the draft through type selection, disabled pending
controls, duplicate Enter/Esc protection, direct detail navigation and retry after
failure. HTTP fixtures cover create payloads, validation rejection and preserving
confirmed creation when the subsequent detail GET fails. No live Jira issues
were created by these checks.
Claude reviewed inline related navigation; its substantive findings were applied
and checked natively. The final follow-up review was attempted but failed because
organizational Claude Code subscription access was disabled. Authenticated
Jira writes still need validation against an account/project; local HTTP
fixtures cover search token pagination, 204 updates and authentication errors.
Title-bar branding was checked in the native demo and the live window's large
icon was extracted to verify the taskbar artwork. That Claude review attempt
was also blocked by the same organizational access error.
The creation-flow visual review was likewise attempted and blocked; its pending,
success and failure screenshots were inspected directly.

Release measurement (2026-11-13): 19,018,752 bytes (18.1 MiB). The release
demo reports 423.88 ms from Rust main to the second-frame callback. This upper
bound is not proof of the 150 ms cached-account startup target; actual first
presentation, production-cache startup and input-latency profiling remain open.
The release executable was launched successfully and its window was responsive.
See design/reviews/release-final.png and design/reviews/startup.txt.

Description-editor Escape check (2026-11-16): a single Escape cancels the edit
even while the Markdown editor has focus, discards unsaved typing as requested,
and restores focus to the same issue view. Pending writes retain their editor;
open field pickers keep their own Escape behavior. The app overrides the editor's
Dismiss key binding because it consumes Escape before raw key handlers.
Native screenshots are `design/reviews/editor-escape-*.png`. Claude found no
visual changes needed; its optional discard confirmation was not adopted because
the requested behavior is single-Escape cancellation, matching the Cancel action.

The command palette keeps its search input focused while Up/Down select matching
commands and issues, skipping group headings. Reveal the selection only at a
viewport edge with `ScrollStrategy::Nearest`; Enter executes that selected row.
Changing the query or reopening the palette selects the first match. Keep issue
ordering stable, hide empty group headings, and preserve direct issue-key lookup
for uncached issues. Shortcut hints are lowercase; issue identifiers retain their
case and use plain monospace text. Reserve a fixed scrollbar gutter and truncate
long titles without shrinking identifiers.
Native checks (2026-11-16) cover arrows across groups, both boundaries, scrolling,
filter focus and reset, selected Enter execution, exact-key opening, empty results
and minimum-width ellipsis. See `design/reviews/palette-*.png`. Claude's initial
and follow-up reviews were completed; substantive layout findings were applied.

Description editor clicks focus the editor and stop mouse-down propagation before
the workspace handler can reclaim focus. The whole bordered area uses the text
cursor; padding and blank-space clicks restore the existing caret, while text
clicks keep the editor's own caret hit testing. Native checks (2026-11-16) cover
defocus/refocus on text, padding and blank space, subsequent typing, and Escape
cancellation followed by reopening the original description. Screenshots are
`design/reviews/editor-refocus-*.png`. Claude reviewed the focus states and found
no visual changes necessary. All 47 tests and Clippy pass.

Rich editor and attachments (2026-11-16): description/comment editors use Zorite's
native Markdown styling with a Rich / Markdown toggle sharing one buffer. Switching
must preserve the draft, selection and undo history. Jira media nodes use protected
`![Jira media](jira-media:N)` references, shown as chips in Rich mode; conversion
restores the original ADF and rejects missing or duplicated references. Other unknown
ADF stays protected. Existing attachments appear below descriptions and editors,
with bounded image previews and browser links. Media Services UUIDs are not Jira
attachment IDs: do not guess inline attachment mappings. Upload/paste of new files
is not implemented. Image fetching uses a bounded worker queue, account-specific
memory entries, size-limited decoding and redirects without cross-origin credentials.
Use `--demo --demo-rich-editor` for the native preview fixture.
Native checks cover both mode states, draft retention across toggles, readable IP
lines, media chips, contained previews and matching file cards. Screenshots:
`design/reviews/rich-editor.png`, `raw-editor.png`, `editor-toggle-draft.png`.
Both Claude design reviews completed; substantive active-state, card and form
layout findings were applied. Final validation: 51 tests pass; Clippy passes with
`-D warnings`. Attachment HTTP fixtures cover the canonical endpoint and invalid
IDs; shared redirect tests cover credential isolation. Live authenticated attachment
downloads and Jira writes were not performed.
The final release build was launched and checked natively; see design/reviews/rich-editor-release.png. The existing user process was preserved.
