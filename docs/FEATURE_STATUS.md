# Feature status

Reviewed 26 September 2026, including multi-file staging selection in the working tree
based on `4a81e31` and guided linear rebase at source checkpoint
`d9af4e6` and the earlier A1–A4 workflows, against the
[product plan](PRODUCT.md). This is the current completion inventory;
[Build status](BUILD_STATUS.md) retains dated commands, results, and artifact evidence.
Instructions for using completed workflows are in the [User guide](USER_GUIDE.md).
The [next-stage plan](NEXT_STAGE.md), scoped with the user on 21–22 September,
orders remote tracking checkout, cherry-pick with conflict guidance, stash, and
hunk staging, with rebase in the following stage. Remote tracking checkout is
implemented; its current platform acceptance is recorded in Build status.
Cherry-pick and shared conflict guidance are also complete on the tested macOS
and Windows platforms. Stash and supported complete text-hunk staging are also
complete on those platforms. Guided linear rebase is also complete on the tested
macOS and Windows platforms, with current packages recorded in Build status.

## Milestone interpretation

The [overnight runbook](OVERNIGHT.md) core tasks F1–F2, B1–B4, P1–P3, G1–G3,
R1–R3, and Q1 have implementation and local macOS/Windows evidence recorded in
Build status. Its M2 ordinary merge/squash-merge and M3 Combine commits/recovery
workflows are also implemented and tested. Folder discovery, easier group
creation, and scoped history search were completed afterward.

**This does not mean every item in the broader product M2/M3 columns is complete.**
Some product-level M1 interaction/packaging expectations and acceptance checks
also remain open, as listed below. Historical milestone completion statements
refer to the delivered runbook workflows on tested platforms.

## Implemented and remaining product features

| Area | Implemented | Outstanding in the product plan |
| --- | --- | --- |
| Launch | Repository/nested-path CLI, JSON project CLI, discovery CLI, help/version, local wrappers, one-window forwarding with navigation guards | Installed PATH integration, further installation polish, additional windows, group templates |
| History | Virtualized paged graph, persistent selected-row highlight, parent continuity, refs, branch/tag scopes, message/body and author/email search, full/prefix commit-ID search, snapshot paging | Advanced revision-query UI |
| Details | Full messages/identities/parents, parent-specific filterable file browser with keyboard selection, collapsible commit information, independently scrolling unified diffs with line numbers/full-row colours, binary/truncation states | Side-by-side diffs, blame and file history |
| Working changes | Staged/unstaged/untracked/conflicted states, whole-file and explicit selected-subset stage/unstage, supported complete text-hunk stage/unstage, staged-only commit, session draft retention, hook/signing failure handling; named stash creation/inspection, retained Apply, reviewed exact-entry Delete and recovery | Individual-line staging; expanded hunk support for excluded file types |
| Remotes | Fetch, clean-only fast-forward pull, explicit normal single-repository push, optional upstream setup | Explicit batch push; hosting integrations only if requested |
| Branches | Local create/switch with previews, explicit remote tracking checkout, ordinary merge, squash merge from local or remote refs, reviewed single non-merge cherry-pick; guided linear rebase onto an existing local or fetched remote branch with Continue, reviewed Skip, Abort, backup and stopped-state guidance | Merge-preserving or arbitrary interactive rebase; multi-commit and merge-commit cherry-pick excluded from A2 |
| Groups | Versioned JSON, create/edit/save/save-as, labels, locate/remove/add, external-edit protection, state/branch/name filters, selection, batch fetch/pull, per-member outcomes/cancel/retry | Explicit group branch switching and further large-group tuning |
| Discovery | Bounded depth-controlled scanning, ordinary/linked worktree detection, reviewed new-project creation and draft additions, duplicate exclusion, partial-result warnings | No further discovery feature required by the current plan; nested/skipped paths require explicit addition |
| Existing-commit squash | Consecutive linear tip selection, editable message, exact-tree replacement, backup refs, journal, guarded recovery including later file edits | No missing feature in the agreed initial Combine commits scope; arbitrary selections are outside this implementation |
| Conflicts | Shared source/target and index-conflict guidance, external-editor resolution, stage resolution, ordinary merge/cherry-pick/rebase continuation and recovery, explicit reviewed Skip, interrupted-marker review, persistent squash review state | Integrated three-way conflict editor |

## Interaction and persistence still outstanding

- The first design pass implements separate Changes/History navigation, a
  dedicated Changes diff pane, an independently scrolling file list and commit
  composer, selected-file highlighting, status/path rows, filtering, keyboard
  file navigation, and section-level stage/unstage with explicit filter scope.
  This working-tree UI has frontend and browser-preview checks, not new native
  package acceptance. A follow-up adds separate checkboxes for staged/unstaged
  versions, Stage selected across unstaged/untracked files, Unstage selected,
  and Clear selection. Filter edits and refreshed snapshots clear the subset;
  conflicts retain individual resolution actions. Broader visual polish remains
  future work. See Build status for current verification and limitations.
- The second design pass adds a compact Workspace menu; a single-row repository
  toolbar with branch-action navigation, icon Refresh, remote selector and
  distinct Fetch/Pull/Push; Stashes navigation; and More for history search and
  Combine commits. Menu dismissal, keyboard navigation/focus return, and retained
  Push fields are implemented. Branch menu items open the existing reviewed
  forms; native OS menus and a redesigned branch-management screen are future work.
- The third design pass adds historical status/path rows, current/previous-path
  filtering, selected-file highlighting and keyboard navigation, a responsive
  file-list/diff split, Show/Hide details, and old/new line-number gutters with
  full-row change colours for the shared diff renderer. Parent changes clear
  stale previews/filters. Git mutation capabilities and preview limits are unchanged.
- The Recent dropdown persists up to 20 successfully opened repositories/projects
  in local webview storage, supports reopening and clearing, and preserves the
  current view on failure. Background project refreshes do not reorder or restore
  cleared entries; unavailable storage falls back to a session-only list with a
  warning. Browser reload/component remount and a full macOS bundle Quit/relaunch
  are verified. Windows full-process restart persistence remains unverified;
  current native-suite results and limitations are recorded in Build status.
- Other persisted view preferences are not implemented. Group filters/selection
  and commit drafts are retained during the session only.
- Repository panels have CSS resize support and adapt their widths. Commit
  details can be hidden to expand the preview; narrow-window navigation collapse
  is still absent.
- Group status reads run with bounded concurrency, but opening/refreshing returns
  a complete snapshot; progressive row delivery during slow reads is absent.
- Repository refresh/search and history navigation shortcuts exist. Group-filter
  focus and the planned shortcut/menu presentation remain incomplete.
- Operation results are available in repository and batch activity sections;
  the planned dedicated activity drawer is not implemented.
- Group table virtualization and larger-scale tuning remain follow-up work.
  Passing the 25-repository measurement does not establish performance at the
  project format's 1,000-member limit.

## Verification and distribution outstanding

Latest collected packages and complete cross-platform backend checkpoint use
rebase source `d9af4e6`: **114 frontend
tests on each platform, 171 Rust tests on macOS / 162 on Windows, and 50 native
workflow checks on each**. The unsigned macOS arm64 app and Windows x64 NSIS
installer were built, launched/installed, and collected with passing reports.
Exact commands, checksums, and historical A1–A4 evidence remain in Build status.

The current redesign/multi-file-selection working tree has **150 frontend tests
passing on each platform**, **51 native workflow checks passing on each
platform**, an updated unsigned Mac app, and a verified Mac Recent quit/relaunch
check. Exact working-tree evidence and incomplete checks are listed separately
in Build status; these checks do not establish full accessibility acceptance or
Windows full-process Recent persistence.

- Linux and Intel Mac execution, and the hosted GitHub workflow, remain unverified.
- Stash requires files ref storage; reftable is unsupported. Safe stash Delete
  is implemented only on macOS and Windows. Changed submodules/gitlinks and
  stashes beyond 1,000 entries or an 8 MiB reflog are excluded from this slice.
- Local packages are unsigned; macOS notarization and public release delivery
  are not configured/completed.
- The [GitHub workflow](../.github/workflows/build.yml) has only manual
  `workflow_dispatch`, with read-only repository permissions. It runs checks and
  builds packages as Actions artifacts. There are no push/PR triggers, native GUI
  acceptance step, or GitHub Release publication job.
- Remote operations are tested against disposable local remotes; real remote-service
  authentication remains unverified.
- Complete screen-reader/Tab traversal and long-group scroll restoration still
  need acceptance checks. Existing navigation/selection checks do not cover these fully.
- Native save-dialog invocation is component-tested with a mock; automated native
  checks exercise actual save operations using fixture paths, not the OS picker itself.
- APFS rejected the invalid raw-UTF-8 filename fixture. Parser and tab/newline path
  tests are covered; that native filesystem scenario remains unverified there.
- Rewrite recovery covers injected failures/interruption states; this is not
  evidence from physical power-loss testing.

No known blocker is recorded for the delivered feature slices. The items above
are remaining features or evidence gaps, rather than passing acceptance claims.

## Explicitly outside initial scope

Cloud accounts, PR review, hosted repository browsing, automatic cloning,
telemetry, terminal emulator, automatic updater, submodule lifecycle management,
worktree creation, arbitrary interactive rebase, force push, hard reset, and
discard UI remain outside initial scope. Existing worktrees are supported;
submodule state can be displayed and its repository opened separately.

## Keeping this inventory current

Every user-visible feature change must update this inventory and the user guide
in the same change. Move an item to implemented only after its code and appropriate
verification exist; keep unverified platform claims explicit. Repository-wide
[contributor/agent instructions](../AGENTS.md) and the
[pull request checklist](../.github/pull_request_template.md) make this part of
the completion/review process. They require documentation maintenance; they do
not automatically generate prose or prove that the guide is current.
