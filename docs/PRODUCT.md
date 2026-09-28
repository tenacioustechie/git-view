# Product and user experience

This document is the product plan, including work not yet implemented. See
[Feature status](FEATURE_STATUS.md) for the completion inventory and the
[User guide](USER_GUIDE.md) for current app behavior.

## Outcome

Git View should make two everyday tasks quick: understand one repository's history and changes, and see which repositories in a project need attention. Opening a repository from a group must preserve the group's filters, selection, and scroll position when returning.

User clarification on 21 September 2026: simple operations happen in both terminal
and visual tools. Complex merges, rebase, and unclear repository state are key
reasons to use a visual client. Prioritize understandable source/target context,
operation state, conflict progress, and next-step/recovery guidance when extending
workflows. Confirmed next-stage sequencing and scope are in the
[next-stage plan](NEXT_STAGE.md).

## Feature milestones

| Capability | M1: overnight core | M2: complete core Git workflows | M3: advanced workflows |
| --- | --- | --- | --- |
| Launch | CLI path, nested working directory, JSON project group | Folder discovery, installation polish, additional windows | Group templates |
| History | Paged DAG, refs, commit selection, branch scope | Search by message/author/OID | Advanced revision query UI |
| Details | Message, identities, parents, changed files, unified diff | Side-by-side diff and richer navigation | Blame and file history |
| Working changes | Staged/unstaged/untracked/conflicted states; file stage/unstage; commit | Hunk staging, stash | Line staging |
| Remotes | Fetch, fast-forward pull, normal single-repo push | Explicit batch push | Hosting integrations if requested |
| Branches | List, create, switch local branch | Remote tracking checkout, normal merge, squash-merge | Rebase and cherry-pick |
| Groups | JSON project files, states, filters, batch fetch/pull | Discovery, explicit group branch switching | Larger-scale tuning |
| Squash existing commits | Specified for later; no placeholder action | — | Tested linear tip squash with backup and recovery |
| Conflicts | Detect and explain in-progress operations | Ordinary merge continue/abort, external resolution | Integrated three-way conflict editor |

Both squash-merge and combining existing commits are confirmed product requirements. M2 merge and squash-merge are the first stretch work after M1 passes; M3 adds combining existing commits, initially a consecutive linear sequence ending at HEAD with backup and recovery. These are separate UI actions: “Squash merge branch” and “Combine commits.” The overnight target is a subset of the full product scope; completing M1 does not complete both squash requirements.

## CLI behavior

```text
git-view                         Open repository containing cwd; otherwise welcome
git-view <path>                  Open repository containing path
git-view repo [path]             Explicit repository mode; path defaults to cwd
git-view group <project-file>    Open repositories from a JSON project file
git-view group --scan <folder>   M2: open discovery results as an unsaved group
         [--depth <n>]           Default depth 3; maximum 10
git-view --help                  Print help without starting the GUI
git-view --version               Print version without starting the GUI
```

Reserve `repo` and `group` as subcommands; ambiguous path names work through `git-view repo ./group`. Resolve relative paths against the invoking process's cwd, including when forwarding to an already-running application. No remote mutation is initiated by a launch command.

Use one app process and one primary window initially. A new CLI launch focuses that window and selects the requested repository/group. Keep commit drafts per repository when switching. If a modal requires a response, queue the incoming navigation request. Retain initial launch requests until the UI acknowledges them. An unreadable/invalid project file or invalid explicit repository path must show an actionable error, not silently open an unrelated repository. Cold-start CLI parse/path errors use a nonzero exit; forwarded requests report delivery at the CLI and any later open failure in the GUI.

GUI opening uses Workspace for repositories/projects and discovery, with an
adjacent Recent dropdown for up to 20 successful opens, newest first. Recent
stores names, kinds, and returned absolute paths locally in webview storage;
reopening uses the normal backend open and navigation guards. Failed opens retain
the current workspace, and Clear recent removes the list only. These controls
are implemented in the working tree; see Build status for verification. Supply
a working local launcher during M1; installed PATH integration for each platform
is part of packaging acceptance.

## Repository screen

Design direction updated 24 September 2026: separate **Changes** and **History**
workspaces. The first working-tree implementation opens in Changes, placing a
filterable file list and commit composer beside a dedicated diff pane. History
retains the branch controls, graph, and commit details pictured below. The second
pass consolidates opening actions under Workspace, keeps remote operations in
a single-row toolbar, adds branch-menu navigation to reviewed forms, moves
Stashes to workspace navigation, and puts history tools under More. The third
pass adds a filterable historical file browser, independent diff scrolling,
Show/Hide details, and numbered, full-row-coloured unified diffs. The next pass
adds explicit Changed files checkboxes and Stage/Unstage selected actions,
separate from file preview, with selection invalidation on filtering/refresh.
See [Feature status](FEATURE_STATUS.md) for
implemented scope and [Build status](BUILD_STATUS.md) for verification; this
working-tree redesign is not yet in the collected native packages.

```text
┌ Repository / group ▾ ── branch ▾ ───────── Fetch  Pull  Push  More ┐
│ Branches / tags │ Working changes (3)       │ Commit / Changes    │
│ Local          │ DAG   Subject   Author    │ Message + metadata  │
│ Remote         │  ●    Fix search   Brian  │ Changed files       │
│                │  ├─●  Feature      Brian  │ Unified diff        │
│                │  ●    Base         Brian  │                     │
├────────────────┴───────────────────────────┴─────────────────────┤
│ Latest operation / expand activity                               │
└──────────────────────────────────────────────────────────────────┘
```

Resizable panels; minimum useful desktop window approximately 1000×680. The View
menu now saves an opening workspace, commit-detail visibility and bounded preferred
panel widths; pointer/keyboard dividers and sliders share those preferences.
Narrower windows fit widths without replacing them. At widths up to 1,000 pixels,
History navigation collapses into a scrollable branch/scope panel opened above
search; branch-menu actions reveal their forms directly. Wide windows restore
the inline sidebar, with inputs and preferred widths retained. One-pane/mobile
navigation remains outside this slice. History is a virtualized list with aligned graph rows. Branch/tag badges
are readable text, and edges retain identity while scrolling and paging. Draw
connectors for unloaded parents so page boundaries do not imply false roots.

Selecting a commit shows full OID, message, author/committer and dates, ordered parents, and changed files. Merge diffs say which parent is being compared and offer a parent selector. Root commits show changes against an empty tree. Binary and truncated diffs have explicit states. Files and messages render as text.

The Changes navigation control, or the pinned Working changes row in History,
opens the Changes workspace. Its Staged, Unstaged, Untracked, and conditional
Conflicted groups share a separate diff pane; the commit composer sits outside
file-list scrolling. A file can be in both Staged and Unstaged. File selection
only previews content; staging is explicit. Group stage/unstage actions state
whether they affect all files or only filter matches, and exclude conflicts.
M1 stages or unstages entire files, shows untracked files as new content, and
commits only staged changes. Retain the draft across workspace navigation and
after hook, signing, or identity failure. For normal commits require staged
content and a nonblank message; an ordinary merge awaiting completion may need
a commit even without an index-to-HEAD diff.

Show detached/unborn HEAD, missing upstream, conflict, and in-progress operations persistently. Disable incompatible actions with a reason. Provide branch create/switch with an explicit target; stale branch selections must be revalidated by the backend.

## Group screen

```text
Group: Work ▾   Add repositories   Search _____   Branch: main ▾
Fetch all   Pull all   Refresh     Selected actions ▾    3 selected

✓ Name       Branch    Working state   Ahead/behind   Last result
✓ api        main      Clean           ↓2             Ready
✓ web        main      2 changed       ↑1 ↓1           Dirty
✓ docs       main      Clean           —               No upstream
  tooling    feature   Clean           ↑0 ↓0           Fetched 2m ago
```

Each row shows name/path, branch or detached state, clean/dirty and file counts, conflict/in-progress status, upstream relation, freshness, and last operation. Unknown or inaccessible is not clean. Clean describes the index and working tree, not remote synchronization. Ahead/behind is last known information; label its age.

Select with checkboxes; click repository name or press Enter to open it. Filters include name/path, exact current branch, dirty, behind, and attention needed. Select all matching means all currently matched rows, not only rendered rows. Show the selected count and any selected rows hidden by filters. A batch uses a fixed selection snapshot; filtering during execution does not change it.

Make Pull all the primary action. It means all group members currently matching the visible filters; with no filters, all members. The label includes the count, and the preview lists every candidate. A separate Pull selected acts only on checked rows, including any selected rows hidden by filters. Pull preview lists eligible and skipped members with reasons. Run eligible members independently, showing queued/running/succeeded/failed/skipped/canceled states. A failed repository does not stop the others. Retry targets failures with a new preflight. Batch cancellation stops queued work and refreshes running repositories; it cannot undo completed remote or local changes.

Groups use explicit JSON project files as defined in [Project files](PROJECT_FILES.md). Paths are relative to the project file by default. The app can open or create a project and add/remove entries; loading a file never clones or executes anything. Missing members remain visible with Locate and Remove from group. Removing membership never deletes repository files.

M2 discovery walks the chosen root to the chosen depth, recognizes `.git` directories and files, skips internal Git data and common dependency/build folders, avoids symlink traversal, and reports inaccessible folders. Stop descending inside a discovered repository by default. Review candidates before saving; rescan proposes additions rather than silently changing membership.

## Interaction and performance

- Respect system light/dark mode, platform modifier keys, text selection, and keyboard focus. Use icons/text alongside color.
- Essential shortcuts: refresh, history row navigation, search/filter focus, open selection, and Escape. The app-bar Shortcuts reference, repository menu hints and project filter/refresh shortcuts are implemented; complete keyboard/screen-reader acceptance remains outstanding.
- Show partial results while loading groups. One slow repository must not block navigation or the other rows.
- Refresh on focus, after operations, and manually. Poll visible group status at a provisional 10-second interval with bounded concurrency; pause when hidden and avoid overlapping scans.
- Keep operation details available in an activity drawer. Normal workflows use action names, with Git output in expanded details.
- Working performance budgets on a documented local fixture: usable shell within 2 seconds warm; first 200 history rows within 1 second after opening; 25-repository status sweep within 5 seconds. These are measurement targets, not established performance claims.
- Render at most the visible history window plus overscan. Fetch no full patches until a commit/file is selected; cap patch size and stored operation output.

## Out of initial scope

Cloud accounts, PR review, hosted repository browsing, automatic repository cloning, telemetry, a terminal emulator, automatic updater, submodule lifecycle management, worktree creation, arbitrary interactive rebase, force push, hard reset, and discard UI. Existing worktrees are valid repository entries; submodule state is displayed, and the submodule can be opened explicitly as its own repository.
