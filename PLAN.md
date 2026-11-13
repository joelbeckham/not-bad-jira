# not-bad-jira — Plan

> A minimalist, keyboard-driven Jira Cloud client for Windows, built with GPUI.
> A desktop successor to `C:\projects\reference\jira-tui`.
>
> Status: draft · 2026-11-13

## 1. Goals

- **Starts super fast.** The window shows real (cached) issues almost immediately, then refreshes in the background.
- **Incredibly snappy.** Every keypress is answered in the next frame. Network work never blocks the UI. Edits are optimistic.
- **Minimalist and well designed.** It looks like a clean, modern, dark Windows app, not a terminal. Every element must earn its place.
- **Keyboard first, mouse friendly.** Every action has a key. Everything you can see can also be clicked.
- **Markdown everywhere.** Descriptions and comments are read and written as Markdown. Converting to and from Jira's ADF format is the app's job, not the user's.

### Non-goals (first pass)

- Ad-hoc JQL search, a global summon hotkey, a split list/detail view
- Editing tabs inside the app (tabs live in the config file)
- An installer or auto-update
- Jira Server or Data Center (Cloud REST v3 only)
- The live-preview (Obsidian-style) editor. It comes in a later pass, but the architecture is ready for it (§7).

## 2. Decisions

| Area | Decision |
|---|---|
| Language / UI | Rust + GPUI via **GPUI Kit** (`gpui-kit` 0.9.x, which pins `gpui-pre =0.3.12`) |
| Look | Dark only. **Inter** for the interface, **JetBrains Mono** for issue keys and code. Custom borderless title bar. Compact density. |
| Layout | Tab bar as the base layer, plus a push/pop view stack (list → detail → drill-in), as in jira-tui. No split view. |
| Config | YAML in `%APPDATA%\not-bad-jira\config.yaml`. Tabs are edited in the file; a command opens it. |
| Credentials | API token in **Windows Credential Manager** (`keyring` crate). Email and base URL live in config. |
| First run | A small setup screen (URL, email, token), plus a one-time import of an existing `.jira-tui/` config if one is found |
| Cache | Disk cache of tab results and issue details for an instant start (stale-while-revalidate) |
| Rich text | Markdown in and out. ADF ↔ Markdown conversion that round-trips content Markdown can't express (§6). |
| Keys | Same vim-style keys as jira-tui, plus **Ctrl+K** for the command palette |
| Mouse | Click to select, double-click to open, clickable tabs, scrolling, right-click context menu |
| Distribution | A single portable `.exe` with an embedded icon and fonts |
| Process | One `AGENTS.md` (conventions, architecture, commands). No spec/ADR folders. |

## 3. Tech stack

| Concern | Choice | Notes |
|---|---|---|
| UI | `gpui-kit` | Inputs, lists and tables that render only visible rows, popovers and menus, Markdown view, theming |
| HTTP | `ureq` (rustls), run on GPUI's background executor | Small, synchronous and simple. No tokio runtime to pay for at startup. |
| JSON | `serde`, `serde_json` | |
| YAML | A maintained serde YAML crate (e.g. `serde_norway`) | Kept as YAML so jira-tui configs import as-is |
| Credentials | `keyring` (Windows Credential Manager backend) | |
| Markdown parsing | `pulldown-cmark` (GFM: tables, task lists, strikethrough) | Used for the Markdown → ADF direction |
| Clipboard / browser | GPUI built-ins (`write_to_clipboard`, `open_url`) | |
| Cache encoding | `serde_json` to start; switch to `postcard` only if profiling says so | |
| Live editor (later) | `zorite-editor` (MIT, `gpui-pre ^0.3`) | Checked during the foundation phase (§8, M0) |

Release profile: `lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"`, `windows_subsystem = "windows"`.

## 4. Architecture

```
src/
  main.rs            app bootstrap, window, fonts, theme, keymap registration
  app.rs             root view: title bar, tab bar, view stack, overlays, toasts
  config/            config.yaml load/save, paths, jira-tui import, credentials
  jira/              REST client, types, endpoints (no UI dependencies)
  adf/               ADF <-> Markdown conversion (pure, heavily unit-tested)
  store/             in-memory issue store, disk cache, refresh/optimistic logic
  views/
    issue_list.rs    a tab's table: columns, sort, quick filter
    issue_detail.rs  fields, rendered description, subtasks, links, comments
    setup.rs         first-run screen
  overlays/
    picker.rs        generic fuzzy picker (status, priority, assignee, type)
    palette.rs       command palette (actions + jump to issue)
    text_edit.rs     title / description / comment editing
    confirm.rs       delete confirmation
  theme.rs           color tokens, spacing, type scale
  actions.rs         every user action as a GPUI action (palette + keymap + menus)
```

**Main principles**

- **The store is the single source of truth.** Views read issues from a shared `Store` entity by key, so an edit in the detail view shows up in the list immediately.
- **One action registry.** Every command is a GPUI `Action`. The keymap, the command palette and the right-click menu all draw from the same list, so they can't drift apart.
- **No blocking work on the UI thread.** HTTP, disk I/O and ADF conversion run on the background executor. Results come back as store updates.
- **The Jira client and ADF converter have no UI code.** They're plain Rust, testable without a window.

## 5. Speed

**Startup path** (target: window painted with cached issues in **under 150 ms**; we measure it and record it in AGENTS.md):

1. Create the window right away. Fonts are embedded, so no font lookup.
2. Read the cache file synchronously. It's small, but cap the size and read lazily if needed. Render the active tab from the cache.
3. In the background: read credentials, call `/myself`, then refresh **all** tabs in parallel. Apply the results as quiet diffs. Keep the selection on the same issue key.
4. No spinners covering content. A thin progress line in the tab bar shows that a refresh is running.

**While running**

- Lists render only the visible rows. Highlight changes re-render a single row's state, not the whole table.
- Opening an issue shows the detail view instantly from list data. The full issue and comments fill in afterwards. Detail data is cached too.
- Optimistic updates for status, priority, assignee, title, description, assign-to-me, done and delete. If the API call fails, roll back and show a toast with the error.
- Fetch dropdown data ahead of time: priorities and issue types once per session. Transitions load when the picker opens, from a short-lived cache. Assignable users are cached on disk.
- Use Jira's newer search endpoint, `/rest/api/3/search/jql` (token pagination), and ask only for the fields the columns need.

## 6. Markdown ↔ ADF

The app works in Markdown. `adf/` converts in both directions.

**Clean round trip:** paragraphs, headings h1–h6, **bold**, *italic*, ~~strike~~, `inline code`, links, code blocks (with language), bullet, numbered and task lists (nested), blockquotes, horizontal rules, hard breaks, simple tables.

**No Markdown equivalent:** mentions, panels, status lozenges, emoji, dates, media and attachments, expand sections, text color, underline, and inline cards.

- **Display:** rendered properly in the detail view (mention chips, colored panel blocks, attachment links, and so on).
- **Editing:** each one becomes a *protected token* in the Markdown text, e.g. `@[Jane Doe](mention:abc123)` or a fenced `:::panel{type=info}` block. On save, the token converts back to the original ADF node, so **an edit never silently deletes content**.
- If the Markdown → ADF step fails, block the save, keep the user's text, and show what failed.

**Tests:** a body of real-world ADF samples, each checked for `adf → md → adf` equivalence. This module is where most of the tests live.

## 7. Live editor (later pass, planned now)

- **Plan:** embed `zorite-editor` in live-preview mode, which shows raw Markdown only near the cursor (Obsidian behavior). It's MIT-licensed and depends only on GPUI (`gpui-pre ^0.3`), so it should resolve to the same 0.3.12 that GPUI Kit pins.
- **Risk:** it's young (about two months, one maintainer). Fallback: fork it, since it's MIT. Reference apps: Velotype, Markion.
- **Ready now:** the first pass uses a plain multi-line Markdown text box behind a small `MarkdownEditor` wrapper. Swapping in zorite later only touches that wrapper. Protected tokens (§6) must survive editing in both editors.
- **M0 checks compatibility:** a throwaway window containing GPUI Kit + zorite-editor, built on Windows. If it fails, we find out now, not later.

## 8. Milestones

Each milestone ends with something you can run.

**M0: Foundation and checks**
- Cargo project, release profile, icon, embedded Inter + JetBrains Mono, dark theme tokens, custom title bar
- Compatibility check: GPUI Kit + zorite-editor in one window, on Windows
- Measure startup time and binary size for an empty window
- `AGENTS.md`

**M1: Config, auth, first run**
- `%APPDATA%` paths, config load, Credential Manager, jira-tui import, setup screen, `/myself` check
- "Open config file" command; reload config when the file changes

**M2: Tabs and issue list** *(the core)*
- Tabs from filter ID or JQL; per-tab columns; parallel load; disk cache; fast startup path
- Navigation (`j`/`k`, Home/End, `1`–`9`, `h`/`l`, ←/→), mouse select and double-click, sorting by column (Shift+N, and clicking headers), `/` quick filter, `r` refresh
- Columns: key, summary, status, priority, assignee, reporter, type, project, created, updated. Priority icons and status colors.

**M3: Issue detail**
- Instant open from list data, then fill in the rest. Fields, rendered description (ADF → display), subtasks, linked issues, parent, comments.
- `enter` to drill into a related issue, `esc` to go back up the stack. Scrolling.

**M4: Editing**
- Pickers: status `s`, priority `p`, assignee `a`. Quick actions: assign to me `i`, done `d`, delete `del` (with confirm). Title `t`.
- Description `e` and comment `m` in the Markdown text box, using the ADF converter with protected tokens
- Optimistic updates, rollback, toasts

**M5: Create, clipboard, browser, palette**
- `c` to create (summary → type → submit, in `default_project`)
- `y` copies the key, `u` copies the URL, `o` opens in the browser
- **Ctrl+K command palette:** every action, plus jump to an issue by key or by text among loaded issues
- Right-click context menu (same actions)

**M6: Polish and ship**
- Empty, error and offline states; a pass on visual detail; key hints in the palette and menus
- Profile startup and input latency against the targets
- Portable release `.exe`

**Later:** live-preview editor (§7), and anything else we decide earns its place.

## 9. Keymap

| Key | Action | Where |
|---|---|---|
| `j` / `k`, ↓ / ↑ | Move | list, detail (scroll / related items) |
| `Home` / `End` | Top / bottom | list, detail |
| `enter` / double-click | Open issue / drill into related issue | list, detail |
| `esc` | Back / clear filter / close overlay | everywhere |
| `1`–`9` | Go to tab N | everywhere |
| `h` / `l`, ← / → | Previous / next tab | list |
| `Shift+1`–`9` | Sort by column N (asc → desc → off) | list |
| `/` | Quick filter | list |
| `r` | Refresh | list, detail |
| `s` `p` `a` | Status / priority / assignee picker | list, detail |
| `t` `e` | Edit title / description | list, detail |
| `i` `d` | Assign to me / mark done | list, detail |
| `del` | Delete (with confirm) | list, detail |
| `c` | Create issue | list |
| `m` | Add comment | detail |
| `y` `u` `o` | Copy key / copy URL / open in browser | list, detail |
| `Ctrl+K` | Command palette | everywhere |
| `q` | Quit | list (with no overlay open) |

Single-letter keys work only when no text input has focus.

## 10. Config

`%APPDATA%\not-bad-jira\config.yaml`. Same shape as jira-tui, minus secrets:

```yaml
jira:
  base_url: https://yourcompany.atlassian.net
  email: you@company.com          # the token is stored in Windows Credential Manager
  default_project: PROJ

cache_ttl: 5m

tabs:
  - label: "My Sprint"
    filter_id: "10042"
    columns: [key, summary, status, assignee, priority]
    sort: priority
  - label: "Recent"
    jql: "project = PROJ AND updated >= -7d ORDER BY updated DESC"
    columns: [key, summary, status, assignee]
```

Cache lives at `%LOCALAPPDATA%\not-bad-jira\cache\`. It's safe to delete at any time.

## 11. Risks

| Risk | Mitigation |
|---|---|
| GPUI / GPUI Kit are pre-1.0 and their APIs change | Pin exact versions; upgrade on purpose, not by accident |
| zorite-editor is young | M0 compatibility check; editor kept behind a wrapper; MIT means we can fork |
| ADF round trip loses content | Protected tokens + a body of round-trip tests; block a save rather than lose data |
| A large or old cache slows startup | Cap the cache size; version the cache format (discard on mismatch) |
| Old search endpoint removed | Use `/search/jql` from the start |
