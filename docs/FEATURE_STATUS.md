# Feature status

Reviewed 22 September 2026, including remote tracking checkout and cherry-pick, against the
[product plan](PRODUCT.md). This is the current completion inventory;
[Build status](BUILD_STATUS.md) retains dated commands, results, and artifact evidence.
Instructions for using completed workflows are in the [User guide](USER_GUIDE.md).
The [next-stage plan](NEXT_STAGE.md), scoped with the user on 21–22 September,
orders remote tracking checkout, cherry-pick with conflict guidance, stash, and
hunk staging, with rebase in the following stage. Remote tracking checkout is
implemented; its current platform acceptance is recorded in Build status.
Cherry-pick and shared conflict guidance are implemented in the working source,
with native/platform acceptance and release packages pending. Stash and hunk
staging remain planned.

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
| History | Virtualized paged graph, parent continuity, refs, branch/tag scopes, message/body and author/email search, full/prefix commit-ID search, snapshot paging | Advanced revision-query UI |
| Details | Full messages/identities/parents, parent-specific changed files, unified diffs, binary/truncation states | Side-by-side diffs, richer file navigation, blame and file history |
| Working changes | Staged/unstaged/untracked/conflicted states, whole-file stage/unstage, staged-only commit, session draft retention, hook/signing failure handling | Hunk staging, stash, line staging |
| Remotes | Fetch, clean-only fast-forward pull, explicit normal single-repository push, optional upstream setup | Explicit batch push; hosting integrations only if requested |
| Branches | Local create/switch with previews, explicit remote tracking checkout, ordinary merge, squash merge from local or remote refs, reviewed single non-merge cherry-pick | Rebase; multi-commit and merge-commit cherry-pick excluded from A2 |
| Groups | Versioned JSON, create/edit/save/save-as, labels, locate/remove/add, external-edit protection, state/branch/name filters, selection, batch fetch/pull, per-member outcomes/cancel/retry | Explicit group branch switching and further large-group tuning |
| Discovery | Bounded depth-controlled scanning, ordinary/linked worktree detection, reviewed new-project creation and draft additions, duplicate exclusion, partial-result warnings | No further discovery feature required by the current plan; nested/skipped paths require explicit addition |
| Existing-commit squash | Consecutive linear tip selection, editable message, exact-tree replacement, backup refs, journal, guarded recovery including later file edits | No missing feature in the agreed initial Combine commits scope; arbitrary selections are outside this implementation |
| Conflicts | Shared source/target and index-conflict guidance, external-editor resolution, stage resolution, ordinary merge and cherry-pick continue/abort, explicit empty cherry-pick Skip, interrupted-marker review, persistent squash review state | Integrated three-way conflict editor |

## Interaction and persistence still outstanding

- Recent repositories/projects and persisted view preferences are not implemented.
  Group filters/selection and commit drafts are retained during the session only.
- Repository panels have CSS resize support and adapt their widths, but the
  planned narrow-window navigation collapse and details toggle are absent.
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

Latest A1 results at `fe33bf3`: **67 frontend tests on each platform, 122 Rust
tests on macOS / 116 on Windows, and 34 native workflow checks on each**.
macOS arm64 app and Windows x64 NSIS packages were built and exercised. Historical
performance measurements and exact commands remain in Build status.

- Linux and Intel Mac execution, and the hosted GitHub workflow, remain unverified.
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
