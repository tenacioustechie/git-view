# Feature status

Reviewed 5 October 2026, including progressive group status in the working tree
based on `600f601`, group branch switching at commit `8c8c3e4`
(following blame commit `9a5ee62`) and guided linear rebase at source checkpoint
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
| Details | Full messages/identities/parents, parent-specific filterable file browser with keyboard selection, collapsible commit information, independently scrolling unified/side-by-side diffs with line numbers/full-row colours, hunk navigation, binary/truncation states; read-only literal-path file history; committed-revision blame with attribution-commit inspection | Syntax highlighting, uncommitted-content blame, automatic rename-following file history and arbitrary-path browsing |
| Working changes | Staged/unstaged/untracked/conflicted states, whole-file and explicit selected-subset stage/unstage, supported complete text-hunk stage/unstage, staged-only commit, session draft retention, hook/signing failure handling; named stash creation/inspection, retained Apply, reviewed exact-entry Delete and recovery | Individual-line staging; expanded hunk support for excluded file types |
| Remotes | Fetch, clean-only fast-forward pull, explicit normal single-repository push, optional upstream setup | Explicit batch push; hosting integrations only if requested |
| Branches | Local create/switch with previews, explicit remote tracking checkout, ordinary merge, squash merge from local or remote refs, reviewed single non-merge cherry-pick; guided linear rebase onto an existing local or fetched remote branch with Continue, reviewed Skip, Abort, backup and stopped-state guidance | Merge-preserving or arbitrary interactive rebase; multi-commit and merge-commit cherry-pick excluded from A2 |
| Groups | Versioned JSON, create/edit/save/save-as, labels, locate/remove/add, external-edit protection, state/branch/name filters, selection, batch fetch/pull; reviewed switching to one shared existing local branch name, fixed member/target snapshots, skip reasons and per-member outcomes/cancel/retry; progressive saved-project opening/refresh with bounded reads, retained rows, stop/retry and generation guards (B2) | Explicit batch push (B3), further large-group tuning; per-member switch targets and remote branch creation excluded from initial B1; create/save still wait for status |
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
- The repository View menu saves an opening Changes/History choice, diff layout, commit-detail
  visibility, and Changed files/Branches/History preferred widths, shared across
  repositories in local webview storage. Pointer/keyboard dividers and sliders
  adjust bounded widths; narrow-window fitting does not overwrite preferences.
  Reset affects display preferences only. Invalid/unavailable storage has safe
  defaults and warnings. Group filters/selection and commit drafts remain
  session-only; window size/position is not persisted.
- C1 adds saved Unified/Side-by-side text previews in Changes, History and Stashes.
  Adjacent removed/added blocks pair by position with blank counterparts; markers
  and old/new numbering remain explicit. Working/history hunk-navigation buttons
  focus block headers; staging still uses backend tokens. Both layouts are capped
  at 10,000 patch lines per preview with an explicit warning; display-limited
  previews disable hunk mutation. Combined headers fall back to unified text.
  Narrow panes scroll instead of changing the preference. Syntax highlighting,
  intraline matching and editable/three-way diffs are not part of this slice.
- C2's initial file-history window uses the selected file's backend path token,
  anchored to the selected commit or working snapshot's HEAD. It pages 200 commits
  at a time with a 30-second page deadline, filters previews by raw path, supports
  real merge-parent comparisons, and labels shallow history. Explicit current/
  previous paths handle a recorded rename; automatic lineage tracking, arbitrary
  path entry and stash launch points are deferred. Reused paths and former
  directory descendants are explained. This is read-only; opening/closing keeps
  the repository workspace and draft. React cleanup rejects stale async results.
- C3 adds read-only Blame from selected working/historical files. It reads an
  exact committed revision (recorded HEAD for Changes), offers the selected
  comparison parent/original path, and navigates each attribution to actual
  commit details and parent-specific patches. Git follows whole-file renames;
  extra copy/move detection and ignore-revision files are not enabled. Raw
  commit identities, explicit shallow boundaries, UTF-8/regular-file limits,
  2 MiB/10,000-line/30-second bounds and stale-result rejection avoid invented
  attribution. The display pages 200 lines; compact commit inspection uses the
  full dialog width. Uncommitted-byte attribution and arbitrary-path/stash
  entry points are excluded from this initial slice. See Build status for
  current verification; full screen-reader acceptance remains outstanding.
- At widths up to 1,000 pixels, History's branch/scope sidebar starts collapsed
  behind a scope button, opening as a scrollable panel. Scope selection,
  Close/Escape and outside interaction dismiss it; direct branch-menu navigation
  reveals the requested form. Inputs and preferred widths survive closing and
  resizing, with focus moved to visible navigation controls when needed. Wide
  windows retain the inline sidebar. This does not implement one-pane/mobile
  navigation or full screen-reader acceptance.
- Saved-project opening, reload and refresh now publish progressive status with
  stable ordered member IDs, at most four concurrent catalog reads, completed and
  unavailable results retained, explicit pending/last-known labels, Stop status
  scan and manual retry. Cancellation and same-revision replacement are guarded
  in the backend and late renderer responses are ignored. Navigation, filters
  and selection remain usable; batch review waits for a complete check.
  Create/save still await their status scan. See Build status for current B2
  test/platform evidence; implementation is not a blanket performance claim.
- The app-bar Shortcuts reference and repository More menu show platform-specific
  keyboard hints. Visible project dashboards support primary-F filter focus,
  primary-R local refresh and Escape clearing only search text. Repository/project
  shortcuts defer to dialogs, IME composition and modified/handled events. Menus
  open at first/last controls with Down/Up, skip hidden/disabled commands, preserve
  native text editing and have targeted Tab/Shift+Tab coverage. Virtual history
  arrow navigation keeps focus on its stable viewport and exposes selected-commit
  live text; removing a focused branch divider moves focus to compact navigation.
  Complete screen-reader and every-workflow keyboard acceptance remain open.
- Repository results now have a dedicated, read-only bottom activity drawer:
  latest/active/attention summary, All/Needs attention filtering, bounded recent
  entries with older active work retained, timestamps and expandable backend
  diagnostics. Keyboard closing restores focus; updates do not open the drawer.
  Records remain backend-session-only. The project Batch activity section keeps
  its existing cancellation/retry controls; a unified project/global drawer and
  persisted operation history are not implemented.
- Group table virtualization and larger-scale tuning remain follow-up work.
  Passing the 25-repository measurement does not establish performance at the
  project format's 1,000-member limit.

## Verification and distribution outstanding

B2 working-tree evidence on `600f601`: 223 frontend tests and 65 native checks
pass on each platform; 191 Rust tests pass on Mac, with separate 15/25/100-member
status measurements. The full Windows backend gate is still incomplete: its
library-test executable exits before assertions with `STATUS_ENTRYPOINT_NOT_FOUND`.
This new loader failure is distinct from the earlier recorded Smart App Control
block. A first Mac native run failed an existing final focus assertion; the
unchanged rerun passed. Exact reports and remaining investigation are in Build
status. The local normal Mac app was rebuilt; collected release packages below
have not been replaced.

Latest collected packages and complete cross-platform backend checkpoint use
rebase source `d9af4e6`: **114 frontend
tests on each platform, 171 Rust tests on macOS / 162 on Windows, and 50 native
workflow checks on each**. The unsigned macOS arm64 app and Windows x64 NSIS
installer were built, launched/installed, and collected with passing reports.
Exact commands, checksums, and historical A1–A4 evidence remain in Build status.

The redesign/multi-file-selection checkpoint `294fd01` has **150 frontend tests
passing on each platform**, **51 native workflow checks passing on each
platform**, an updated unsigned Mac app, and a verified Mac Recent quit/relaunch
check. Exact working-tree evidence and incomplete checks are listed separately
in Build status; these checks do not establish full accessibility acceptance or
Windows full-process Recent persistence.

The view/panel preferences working tree based on `294fd01` passes **165 frontend
tests and 53 native workflow checks on each platform**, including native WebView
reload persistence and bounded 800×580 panel fitting. An updated unsigned Mac app
was built; installed apps were not replaced. Browser preview verified pointer
resizing and controls. Full OS-process restart persistence of these new
preferences remains unverified on both platforms; this is separate from the
earlier Mac Recent restart check.

The 27 September compact-navigation working tree on the same baseline passes
**169 frontend tests and 54 native workflow checks on each platform**. Native
checks cover the compact panel at 800×580; browser inspection also verifies
form retention and focus when widening to 1280×800. The Mac bundle was rebuilt;
installed apps and collected packages remain unchanged. See Build status for
the earlier incomplete runs and final-source reports.

The keyboard/accessibility working tree based on `fa49114` passes **183 frontend
tests and 56 native workflow checks on each platform**. Targeted browser checks
cover actual Tab/Shift+Tab, the shortcut reference, project search, menu navigation
and responsive focus. Native checks cover the same product source; subsequent
Mac harness-only isolation/timing adjustments and exact reports are documented
in Build status. This does not establish full VoiceOver/NVDA acceptance or an
exhaustive keyboard-only pass through every workflow. The unsigned Mac bundle
was rebuilt; installed apps and collected packages remain unchanged.

- Linux and Intel Mac execution, and the hosted GitHub workflow, remain unverified.
- The repository activity-drawer working tree on `dd38547` passes **189 frontend
  tests and 57 native workflow checks on macOS**, with a rebuilt unsigned Mac app.
  Its initial Windows verification stalled before producing test output. The
  subsequent C1 gate below verifies the drawer on Windows too. Full screen-reader
  acceptance remains open.
- The C1 side-by-side working tree on `7703dc3` passes **202 frontend tests and
  59 native workflow checks on each platform**. Native checks cover
  historical alignment/read-only behavior, working hunk focus and exact staging,
  unstaging and partial-commit contents, plus layout persistence across WebView
  reload. The Mac retry passed after an incomplete 21-check timeout; both reports
  are recorded in Build status. The unsigned Mac bundle was rebuilt; installed apps and collected
  packages remain unchanged. Full OS-process restart persistence of the new diff
  preference and screen-reader acceptance remain unverified.
- C2's initial literal-path file-history working tree on `5df7c24` passes **208
  frontend tests, the full Rust suite (one explicit performance measurement
  ignored), and 60 native workflow checks on both macOS and Windows**. Browser review covers a
  compact 800×580 dialog and targeted keyboard/focus paths. The unsigned Mac
  bundle was rebuilt; installed apps and collected packages remain unchanged. Automatic
  rename following and full screen-reader acceptance are not claimed.
- C3's committed-revision blame working tree on `9266e9b` passes **214 frontend
  tests and 61 native workflow checks on each platform**, with full Rust suites
  (182 Mac / 173 Windows passed; one separate performance measurement ignored).
  The final compact-layout focus adjustment is included in both native runs;
  the 800×580 browser visual/keyboard review and exact reports are in Build status. The unsigned Mac bundle
  was rebuilt; installed apps and collected release packages were not replaced.
  Full screen-reader acceptance and uncommitted-byte attribution are not claimed.
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

B1 group switching, now committed as `8c8c3e4`, passes **219 frontend tests on each platform**,
**187 Mac Rust tests** (one separate measurement ignored), and **63 native
checks on each platform**, with a rebuilt unsigned Mac bundle. All five new group integration
tests pass on Windows, but its full Rust gate stops at an Application Control
block (OS error 4551) before the cherry-pick integration binary runs. A subsequent
read-only event-log check confirms Smart App Control's enforced
`VerifiedAndReputableDesktop` policy blocked the unsigned test binary. No security
policy was changed. The separate Windows native run also passes strict Clippy,
lint/typecheck and smoke builds. Exact evidence is recorded in Build status;
this partial backend run is not a complete Windows Rust-suite pass.

The items above are remaining features or evidence gaps, rather than passing
acceptance claims. The Windows Application Control gate is an explicit outstanding
verification blocker, not a known group-switch behavior failure.

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
