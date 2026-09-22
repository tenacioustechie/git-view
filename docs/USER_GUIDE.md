# Git View user guide

Reviewed 22 September 2026 for version 0.1.0, including the rebase controls at
source checkpoint `f46bf79`. The collected macOS and Windows packages remain at
the A4 checkpoint `9fe0fb8` until the rebase platform gate is recorded in
[Build status](BUILD_STATUS.md).
This guide describes implemented behavior. See [Feature status](FEATURE_STATUS.md)
for the remaining roadmap and [Build status](BUILD_STATUS.md) for tested versions,
packages, and platform limitations.

## Contents

- [Start the app](#start-the-app)
- [Open a repository](#open-a-repository)
- [Create a group with folder discovery](#create-a-group-with-folder-discovery)
- [Create and edit project files](#create-and-edit-project-files)
- [Check and update a group](#check-and-update-a-group)
- [Browse and search history](#browse-and-search-history)
- [Stage files and commit](#stage-files-and-commit)
- [Fetch, pull, and push](#fetch-pull-and-push)
- [Create and switch branches](#create-and-switch-branches)
- [Cherry-pick a commit](#cherry-pick-a-commit)
- [Rebase a branch](#rebase-a-branch)
- [Save and restore work with stashes](#save-and-restore-work-with-stashes)
- [Merge a branch](#merge-a-branch)
- [Squash merge a branch](#squash-merge-a-branch)
- [Combine existing commits and recover](#combine-existing-commits-and-recover)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Troubleshooting and current limits](#troubleshooting-and-current-limits)

## Start the app

Git View works with existing local Git repositories. Install Git and configure
your usual Git identity, credentials, hooks, and signing outside the app. Git View
uses those settings; it has no separate hosting-account login or clone screen.

The current local packages are a macOS arm64 app archive and a Windows x64
installer. Find their paths and checksums in [Build status](BUILD_STATUS.md).
Extract the macOS archive and open **Git View.app**, or run the Windows installer
and launch the installed app. These packages are unsigned; macOS notarization is
also outstanding. Linux and Intel Mac execution have not been verified.

To build from this repository, first follow [Development machine setup](../Dev-Machine-Setup.md),
then run from the checkout:

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

For a release build, use `pnpm tauri build`. The local launcher uses a release
executable when available, otherwise a debug executable:

```sh
scripts/git-view .
scripts/git-view repo ~/code/api
scripts/git-view group ./work.git-view.json
scripts/git-view group --scan ~/code --depth 3
```

On Windows, use `scripts\git-view.cmd` in place of `scripts/git-view`, for example:

```powershell
.\scripts\git-view.cmd repo C:\codeme\api
.\scripts\git-view.cmd group C:\codeme\work.git-view.json
.\scripts\git-view.cmd group --scan C:\codeme --depth 3
```

The wrappers are local to this checkout and are not installed on PATH. To launch
from another directory, use the wrapper's full path; relative arguments resolve
against your terminal's current directory. Rebuild after source changes to avoid
launching an older release executable.

## Open a repository

Choose **Open repository** and select a repository folder or a folder inside it.
The app finds the containing worktree. Existing linked worktrees can also be
opened. Opening reads local state; it does not fetch or pull.

The CLI accepts the following arguments after the launcher or executable:

| Arguments | Result |
| --- | --- |
| No arguments | Open the repository containing the current directory, or show welcome if there is none |
| `<path>` or `repo <path>` | Open the repository containing that path |
| `repo` | Open the repository containing the current directory |
| `group <project-file>` | Open a saved JSON project |
| `group --scan <folder> [--depth <n>]` | Open the discovery screen; choose **Scan repositories** to scan |
| `--help` / `--version` | Print information without opening the GUI |

Quote paths containing spaces. Use `repo ./group` for a repository whose name
could be mistaken for a subcommand. A second launch focuses the existing primary
window; navigation waits while an action confirmation or native dialog is open.

## Create a group with folder discovery

A **project** is a saved JSON file describing a **group** of repositories.

1. Choose **Discover repositories** in the top navigation and select the folder
   containing your checkouts.
2. Set **Search depth**. The default is 3; 0 checks only the root folder, and the
   maximum is 10. Choose **Scan repositories**.
3. Review the paths and any scan warnings. Check the repositories to include, or
   use the header checkbox to select all available results. New results start
   unchecked.
4. Enter a **Project name**, choose **Save selected as project (N)**, and choose
   a JSON filename such as `work.git-view.json`.
5. The saved project opens as a dashboard. Click a repository's name to open it;
   use **Back to …** to return to the group.

Discovery recognizes `.git` directories and linked-worktree `.git` files. It
stops descending once it finds a repository, skips symlinks and common build or
dependency folders, and reports inaccessible folders. A scan can return partial
results when a time or size limit is reached; scan a smaller root if needed.
Nested repositories beneath a discovered repository need to be added explicitly.

**Rescan** retains selections still present and leaves newly found repositories
unchecked. **Cancel discovery** leaves saved membership unchanged. Scanning and
checking boxes do not save a project by themselves.

To add discovered repositories to an existing project, use **Discover repositories**
in that project's dashboard header, then **Add selected to draft (N)** and
**Save project**. The top navigation discovery button creates a new project.
Repositories already in the project or discovery draft are marked and excluded
from additions.

## Create and edit project files

For an empty project, enter **New project name** on the welcome screen and choose
**Create project**, then a save location. If a repository is open without a
project, clicking **Git View** returns to welcome. If a project is already open,
that link returns to its dashboard; use discovery to create another populated
project, or launch without arguments from outside a repository to show welcome.

On a project dashboard:

- **Add repositories** opens a folder picker and adds entries to an editing draft.
- **Edit project** lets you change the project name and repository labels.
- **Locate** replaces a missing or moved entry's path.
- **Remove** removes membership only; repository files remain on disk.
- **Save project** writes the draft. **Save as** writes a separate project and
  adjusts relative paths for the new location. **Cancel editing** discards the draft.

Entries keep their order. You can also edit the JSON directly:

```json
{
  "version": 1,
  "name": "Work",
  "repositories": [
    { "path": "./api", "label": "API" },
    { "path": "./web", "label": "Web" },
    { "path": "../shared-library" }
  ]
}
```

Paths are relative to the project file's directory. Environment variables and
`~` are not expanded inside JSON. Use forward slashes for portable relative paths;
absolute paths are machine-specific. Missing repositories stay visible with an
explanation. See [Project files](PROJECT_FILES.md) for the schema and limits.

If the file changes externally while you are editing, Git View refuses to
overwrite it. **Reload project** loads the disk version and discards the current
draft; use **Save edits as** first if you want to keep those edits separately.
A malformed external edit leaves the last valid dashboard visible and blocks
new batches until the project is repaired/reloaded or the edits are saved separately.

## Check and update a group

Rows show the branch, working state, last known upstream counts, freshness, and
last operation result. **Clean** describes local files and index state; a clean
repository may still be ahead of or behind its upstream. Unknown or unavailable
repositories are not reported as clean. Ahead/behind counts can be stale until
you fetch.

Use **Search** for a name/path, **Branch** for an exact current branch, and
**Working state** for dirty, behind, or attention-needed repositories. A branch
filter does not switch branches. **Clear filters** shows all members.

| Action | Scope |
| --- | --- |
| **Pull all (N)** / **Fetch all (N)** | All rows matching the current filters |
| **Pull selected (N)** / **Fetch selected (N)** | All checked rows, including checked rows hidden by filters |
| Header checkbox | Select or deselect all currently matching rows |

1. Choose the appropriate fetch or pull action.
2. Review the fixed candidate list, destinations, and skip reasons.
3. Choose **Run N eligible**. Each repository is checked again before execution.
4. Read **Batch activity** for each result. One failure does not stop other members.

Pull is fast-forward-only and skips dirty repositories, including untracked
files. It does not automatically stash, rebase, or create a merge commit. Missing
upstreams and incompatible Git operations also prevent pulling.

To retry, select failed, skipped, or canceled entries in **Batch activity** and
choose **Preview retry (N)** after the batch finishes. Fix the reported cause
first. **Cancel batch** stops queued work and requests cancellation of running
work; completed changes remain. An uncertain outcome requires a refresh and
inspection before retrying.

Visible group status refreshes on focus and approximately every ten seconds;
**Refresh** is also available. Filters and selection survive opening a member
and returning. These view preferences are not persisted across app restarts.

## Browse and search history

The repository view has history scope controls, a graph/list, and details.
Choose **All branches and tags**, a local branch, a remote branch, or a tag to
set the history scope. Selecting a scope does not check out that branch.

Select a commit to see its full message, ID, author/committer, parents, and changed
files. Select a file to show its unified diff. For a merge commit, use **Compare
with parent** to select the comparison; a root commit is compared with an empty
tree. Binary or truncated previews are explicitly labeled. Use **Load older
commits** for another page.

To search the selected scope, choose **Search by**, enter a query, and choose
**Search history**:

| Search by | Matches |
| --- | --- |
| **Message** | Literal, case-insensitive text in the full commit message, including its body |
| **Author** | Literal, case-insensitive text in author names or email addresses |
| **Commit OID** | A full commit ID or a hexadecimal prefix of at least four characters |

Search is limited to commits reachable in the chosen scope. If a prefix is
ambiguous or too broad, enter more characters. Queries must be nonblank, a single
line, and at most 512 UTF-8 bytes; searches have a 30-second limit. There is no
regular-expression or arbitrary revision-query syntax.

Results support the same commit and file inspection and **Load more results**.
Graph connectors are omitted between filtered results to avoid implying false
parent relationships. **Clear search** restores the graph. Refresh if Git View
reports an expired history snapshot. Historical views ignore replacement refs
and reject legacy grafts.

## Stage files and commit

1. Select **Working changes** above the history list.
2. Inspect the **Staged**, **Unstaged**, **Untracked**, and **Conflicted** sections.
   Select a file to inspect its available diff.
3. Choose **Stage** for each whole file to include. **Unstage** removes a file's
   staged changes from the next commit while keeping its working-file content.
4. Enter a nonblank **Commit message**.
5. Choose **Commit staged changes (N)**.

A file can have both staged and unstaged changes. Only staged content is
committed; later unstaged edits remain. Normal commits require staged changes
and no unresolved conflicts.

Commit hooks and signing remain active. On failure, read the error, resolve the
cause using your normal Git setup, and retry. Draft messages survive failure and
navigation between repositories during the session, but are not saved across
app restarts. An ordinary merge uses **Continue merge**, described below.

### Stage or unstage one hunk

Hunk staging is available in the collected macOS and Windows A4 packages. Its
native acceptance and package checks are recorded in [Build status](BUILD_STATUS.md).

A **hunk** is a complete change block in a text diff. To include one block in
the next commit:

1. In **Working changes**, select a tracked file from **Unstaged**.
2. Check the file path and **Diff · unstaged** heading. Inspect the numbered
   blocks and choose **Stage hunk** beside the desired block.
3. Wait for the refreshed changes. Select the file under **Staged** to review
   exactly what will be committed; other edits remain under **Unstaged**.
4. Enter your message and use **Commit staged changes (N)** normally.

To remove one staged block from the next commit, select the file under **Staged**
and choose **Unstage hunk** beside that block in **Diff · staged**. Other staged
blocks remain. Hunk actions change the index; the app does not rewrite your working
file. Existing whole-file **Stage** and **Unstage** controls remain available.
Your commit draft is retained through hunk actions and failures.

These actions apply to complete hunks in existing regular tracked text files
with unchanged file mode. Supported cases include line insertions/deletions,
emptying a file, LF/CRLF, Unicode paths, and a missing final newline. There is no
individual-line selection. New/deleted files, renames, mode/type changes, binary
files, symlinks, submodules, unresolved conflicts, special index flags, unsupported
filters/encodings, and incomplete previews show an unavailable reason. Use the
whole-file controls where appropriate. Each source blob and diff preview is
limited to 2 MiB for this action.

If another edit or Git action makes the displayed hunk stale, refresh and inspect
the fresh diff before retrying. Never assume that a same-looking block still
identifies the earlier change. Normal index hooks remain active; unexpected
hook/external changes are reported for inspection without an automatic rollback.

## Fetch, pull, and push

- **Fetch:** select a **Remote**, choose **Fetch**, review the preview, and run it.
  Fetch updates remote-tracking information without merging into the current branch.
- **Pull:** choose **Pull**, review the configured upstream, and run it. Pull uses
  the current branch's upstream, which may differ from the toolbar's remote
  selection. It requires a clean worktree and fast-forward-compatible history.
- **Push:** select a **Remote**, expand **Push…**, and check **Destination ref**
  (for example, `refs/heads/main`). Optionally select **Set upstream**. Choose
  **Preview push**, inspect the destination, then run the action.

Push is a normal single-repository push. There is no force-push or batch-push
control. A non-fast-forward rejection must be resolved before retrying. Network
operations use the installed Git's credential helpers and SSH configuration;
remote-service authentication still needs verification in your own environment.

## Create and switch branches

Start with a clean index and worktree and finish any in-progress Git operation;
both branch creation and switching require this in Git View.

Expand **Switch branch**, select a local branch, choose **Preview switch**, and
confirm the target. Git View rechecks repository state before execution.

To create a branch, select the desired starting commit, expand **Create branch**,
enter **Name**, and optionally check **Switch after creating**. **Preview create**
shows the action. The starting point is the selected commit, or HEAD when no
commit is selected; inspect the **Starts at** text before confirming.

To start working on a remote branch, expand **Check out remote branch**:

1. Choose **Remote branch** from locally recorded candidates. Use **Fetch** first
   if the branch is missing or outdated; opening the form does not fetch.
2. Review the remote, source ref/commit, and intended upstream. Edit **Local branch
   name** if desired; your typed name is retained during refresh.
3. Choose **Preview checkout**, inspect the exact source and current HEAD, then
   **Create and switch**. The app creates the local tracking branch and refreshes
   its actual branch/upstream state.

If the name already exists, choose another name or **Review existing local branch**
to open the separate switch controls. Existing branch tips and upstreams are not
reset. Branch namespaces or leftover tracking configuration can also block a
name. Dirty worktrees, untracked changes, and active Git operations must be
resolved first. Ignored files that checkout would overwrite are protected.

Candidates require an unambiguous configured fetch mapping to a remote branch.
Symbolic remote HEAD entries are excluded. Unsupported mappings are explained;
remote branches mapping to multiple local refs, or into local heads/tags/app refs,
cannot use this action. Preview state is checked again at execution. Hooks run
normally. A hook failure may occur after checkout has taken effect; inspect the
refreshed branch and activity before retrying. No automatic rollback deletes a
partially created branch or discards working files.

Remote refs can also be inspected or used as merge sources. Group-wide branch
switching is not implemented.

## Cherry-pick a commit

Use this to apply one existing commit's changes to the branch currently checked
out. Begin on a local branch with an existing commit, a clean index/worktree,
and no operation in progress.

1. Select the source commit in history. You can change history scope to find it;
   the scope does not change the checked-out target branch.
2. Inspect the source message, author, parent, and changed files, then choose
   **Cherry-pick commit**.
3. Review the source commit and current target branch/HEAD in the preview, then
   confirm. A successful pick adds one commit using the source message and author.
4. Inspect refreshed history and working changes. No push follows automatically.

Merge commits, multi-commit sequences, and group-wide picks are outside this
action. An initially empty source commit is blocked. If the source changes are
already present and Git stops with no remaining changes, choose **Skip empty
cherry-pick** to finish without a new commit, or review **Abort cherry-pick**.
An unfinished or empty pick is not reported as a successful new commit.

When conflicts occur, **Cherry-pick in progress** shows the current target,
known source, unresolved paths, and next steps. Open those files in your editor,
resolve them, return to Git View, **Refresh**, and choose **Stage resolution**.
Editing conflict markers alone does not resolve Git's index entries. Review the
staged changes, choose **Continue cherry-pick**, and confirm. The ordinary commit
button is unavailable during a cherry-pick.

**Abort cherry-pick** requires a separate review: Git may discard edits made
during resolution. Preserve any edits you want to keep before confirming. The
operation state survives reload; after external Git changes, refresh and inspect
the reported context. An externally started pick may have unknown original target
information: its Abort preview describes the reviewed current HEAD instead.
Unknown or inconsistent state can require manual Git recovery instead of an app
action. If **Clear interrupted cherry-pick** is offered for a clean, unchanged
target, review it to remove only the app's interrupted marker; it does not reset
files or move the branch. Multi-commit sequences started externally must be
managed externally.

If a recorded cherry-pick conflicts with a different active Git operation, the
panel keeps the actual operation label and displays the old record separately.
Mutations remain blocked until that mismatch is reviewed and resolved externally;
inspection, refresh, and fetch remain available.

Git's own hook/signing behavior applies. An initial cherry-pick and **Continue
cherry-pick** can invoke different hooks. If a hook or signing fails, inspect the
current state, fix the reported configuration, then use the available recovery
action rather than starting another pick. Ignored-content collisions are blocked;
some directory-rename situations require preserving or moving ignored content
outside the worktree before retrying.
Cherry-pick and its recovery actions do not recursively update submodule
worktrees; manage submodule contents separately.

## Rebase a branch

Use **Rebase branch** to replay the current local branch's linear commits onto
an existing local or fetched remote branch. Begin on a clean, attached local
branch with an existing commit and no Git operation in progress. Untracked and
submodule changes also block the start. Move ignored files outside the worktree
before rebasing; the initial workflow blocks them because directory renames can
put them at paths Git might overwrite.

1. Expand **Rebase branch** in the branch controls and choose **Onto branch**.
   Use **Fetch** separately if a remote branch is missing or stale. Selecting
   a history scope does not change the current branch or the rebase target.
2. Choose **Preview rebase**. Check the current branch and original HEAD, exact
   target ref and commit, merge base, and every commit listed in replay order.
3. Confirm **Run rebase branch**. Git View rechecks that the preview is still
   current, saves a backup ref under `refs/git-view/rebase-backups/`, and starts
   the replay. The source commits receive new IDs. No fetch or push follows.
4. If Git stops, read **Rebase in progress** for the original branch/HEAD,
   chosen base, current source commit, progress, unresolved files, and available
   controls. Resolve conflicts in an external editor, return, **Refresh**, and
   **Stage resolution**. During rebase, Git's “ours” is the chosen base plus
   commits already replayed; “theirs” is the commit being replayed.
5. When the conflict is fully staged, review **Continue rebase**. Git uses its
   recorded commit message and normal hooks/signing. It may stop again at a
   later commit. Your ordinary commit draft stays separate and is not used.

**Skip commit** has a separate confirmation naming the commit. It omits that
commit's change from the resulting branch and may discard edits made while
resolving it. Use it only after reviewing those changes. A commit that becomes
empty can stop for review; Continue may still drop it, so the app offers Skip
or Abort rather than promising to preserve it. Use Git externally if you need
to retain an empty commit or edit its message.

**Abort rebase** has a separate confirmation. Git attempts to restore the
original branch/HEAD and may discard resolution edits. Inspect actual refs and
files afterward, especially if Git reports an error. The backup ref remains
after completion or Abort for manual recovery; there is no one-click reset to
it. If the recorded rebase metadata no longer matches Git, the app disables its
recovery controls and asks for manual review. A rebase started outside Git View
is displayed for context but must be continued, skipped, or aborted with Git.

The initial workflow accepts up to 100 linear source commits with exactly one
merge base. It excludes merge commits in the source range, shallow or grafted
ancestry, replacement refs, symbolic remote HEADs, detached/unborn branches,
automatic fetching, interactive rebase, and force push. A normal push can reject
a rebased branch whose old commits were published; Git View has no force-push
control. See [Feature status](FEATURE_STATUS.md) for platform verification.

## Save and restore work with stashes

The **Stashes** button opens saved work for the current repository. Linked
worktrees share this list. See [Build status](BUILD_STATUS.md) for the verified
source and packages.

To set work aside:

1. Enter a nonblank **Stash message**.
2. Leave **Include untracked files** unchecked to save only tracked staged and
   unstaged changes, or check it to also save newly created files. Ignored files
   are always excluded.
3. Choose **Create stash**, review what will be saved and removed, then confirm.
4. Inspect the refreshed working changes. Untracked files left out of the stash
   remain on disk and can keep the repository dirty.

Creation requires an existing commit, no unfinished Git operation or conflicts,
and eligible changes. Submodule changes must be managed separately. Your stash
message is retained during the session, independently of your commit draft.

Select a saved entry to inspect its message, date, full commit ID, and originating
branch when known. **Inspect saved changes** offers **All tracked changes**,
**Saved staged changes**, and, when present, **Saved untracked files**. Select a
file to read its bounded diff. Refreshing an unchanged list keeps the selected
entry. A refresh that changes entry identity clears the selection; select and
review the entry again before acting.

To restore work, first make the destination clean, including untracked files and
submodules. Select the entry, optionally check **Restore staged state**, choose
**Apply stash**, review the current target and saved entry, then confirm. Apply
always keeps the saved entry. Restoring staged state requests the saved index
as well as working-file contents. With that checkbox unchecked, Git's default
behavior still stages some newly added files; inspect the resulting index.

An Apply error can leave some changes applied. Read the result and refresh before
retrying; there is no automatic undo. If **Unresolved index conflicts** appears,
resolve files in your external editor, return, **Refresh**, and choose **Stage
resolution**. Once all conflicts are resolved, review and use the normal commit
controls. Stash conflicts do not create an ordinary merge operation, so the app
does not offer ordinary merge Continue or Abort for them. Unknown conflict
origins are explicitly shown as unknown.

**Delete stash** is a separate reviewed action. It removes only the selected
saved entry and does not undo applied working changes. Canceling its preview
keeps the entry. A private recovery backup retains the saved object after Delete;
this is not permanent erasure of its contents. There is no Pop or bulk-clear
control. A stale list must be refreshed and reviewed again.

If **Interrupted stash deletion** appears, read its message and backup reference.
**Review deletion recovery**, when available, previews completion of the recorded
Delete. Confirming may finish removing the entry; it does not restore working
files. The backup remains. If recovery is unavailable because external metadata
differs, preserve the journal and backup and review the repository with Git
instead of repeatedly retrying. Merely opening or refreshing the list does not
repair it.

The initial stash implementation supports Git's files ref storage, with at most
1,000 entries and an 8 MiB stash reflog. Reftable storage is unsupported. Reviewed
Delete uses platform-specific atomic replacement on macOS and Windows; it is
unavailable on Linux. Applying stashes that change submodule gitlinks is excluded.
Ignored-file collisions and potentially unsafe directory-renaming cases are
blocked before Apply. Use **Close stashes** to return to the other repository
controls.

## Merge a branch

1. Switch to the target local branch and ensure the index and worktree are clean.
2. In **Merge branches**, choose **Source branch** and **Merge branch**.
3. Review the source and current target, then confirm the action.

An ordinary merge may fast-forward or create a normal merge commit. If conflicts
remain, open the files in your editor, resolve them, return to Git View, refresh,
and choose **Stage resolution** for each resolved file. Once all conflicts are
staged, choose **Continue merge** and confirm. This uses the merge's recorded
message and normal hooks/signing.

The operation panel shows the current target, available source commit context,
unresolved files, and next steps. A source or original target that cannot be
identified reliably is shown as unknown rather than inferred from history scope.

**Abort merge** is available for an ordinary merge in progress. Review your
resolution edits before confirming: Git attempts to restore the pre-merge state
and may discard those edits. Inspect the result if Git cannot complete the abort.

## Squash merge a branch

Use this to bring another branch's changes into one new commit on the current
branch.

1. Start on a clean target local branch.
2. In **Merge branches**, choose **Source branch**, then **Squash merge branch**.
3. Review and confirm. The result remains staged for review.
4. Resolve any conflicts in your editor, refresh, and **Stage resolution**.
5. Review **Working changes**, enter the final **Commit message**, and choose
   **Commit staged changes (N)** to create the separate normal commit.

The squash operation does not create that commit automatically. Its review state
survives reload. A squash merge has no ordinary merge continuation or abort;
abandoning its changes requires deliberate review and cleanup with Git outside
the app. The app has no discard/reset UI.

## Combine existing commits and recover

Use **Combine commits…** to replace commits already on the current branch with
one commit. This changes their commit IDs while retaining the final tree.

1. Start on an existing local branch with a clean index and worktree and no Git
   operation in progress.
2. Choose **Combine commits…** and set **Newest commits to combine** to at least 2.
   The selection is a consecutive sequence ending at HEAD. It cannot contain a
   merge commit, root commit, or shallow boundary, and is limited to the loaded
   current-branch history shown in this panel.
3. Review the selected IDs and replacement parent, then edit **Combined commit message**.
4. Choose **Preview combine commits**, review the rewrite warning, and confirm.
5. Inspect the resulting branch history and **Backups and recovery** record.

Git View creates backup refs and a recovery record, runs hooks/signing while
preparing the replacement, and rechecks the branch before installing it. A
successful replacement keeps the original final tree. Unsupported repository
configurations, including sparse checkouts and some signing/configuration
contexts, are rejected with a reason. Backup/replacement refs and the preparation
worktree are retained for recovery; there is no automatic cleanup workflow.

Remote-tracking information cannot prove that the selected commits are
unpublished. Combining previously shared commits can make a normal push fail;
Git View does not force-push them.

To restore the original tip, reopen **Combine commits…**, find its record under
**Backups and recovery**, choose **Preview recovery**, and confirm. Eligible
recovery restores the original tip while preserving later working-file edits.
A later branch commit, switched branch, changed backup, or inconsistent record
can block automatic recovery; inspect the displayed refs and seek manual review
instead of resetting files. See [Recovery design](REWRITE_RECOVERY.md) for details.

## Keyboard shortcuts

| Context | Shortcut | Action |
| --- | --- | --- |
| Repository view | `⌘R` on macOS / `Ctrl+R` elsewhere | Refresh |
| Repository view | `⌘F` / `Ctrl+F` | Focus and select the history query |
| History search input or history/results list | `Escape` | Clear the query/search |
| Focused history/results list | `↑` / `↓` | Select the previous/next loaded commit |
| Focused history/results list | `Home` / `End` | Select the first/last loaded commit |

Repository shortcuts defer to open confirmation dialogs. Native buttons and
inputs can be reached with the keyboard; complete screen-reader and Tab traversal
acceptance remains outstanding. A group-filter focus shortcut is not implemented.

## Troubleshooting and current limits

| Situation | What to do |
| --- | --- |
| Repository missing from discovery | Check scan depth and warnings; try a smaller root or **Add repositories**. Nested checkouts and skipped folders may need explicit selection. |
| Project member unavailable | Use **Locate**, save the corrected path, or check filesystem access. Removing it only changes project membership. |
| Pull skipped as dirty | Review staged, unstaged, untracked, and conflicted files. Commit or otherwise resolve them before previewing another pull. |
| Missing upstream or non-fast-forward pull | Inspect branch/upstream state. Configure the intended upstream or reconcile history explicitly with Git; pull does not do this automatically. |
| Preview or history snapshot expired | Refresh, inspect current state, and create a new preview/search. |
| Hook, signing, identity, or credential failure | Correct the installed Git configuration, then retry. Read the operation error; do not assume the action completed. |
| Operation canceled or outcome uncertain | Refresh and inspect local/remote state before retrying; cancellation does not undo completed work. |
| External project edit conflicts with a draft | Save the draft separately with **Save edits as**, or **Reload project** to use the disk version. |

Not yet available: individual-line staging, side-by-side diffs, blame/file history,
interactive rebase, batch push, group branch switching, integrated conflict editing,
multiple windows, group templates, and persisted recent-project/view settings.
The broader backlog and verification gaps are tracked in [Feature status](FEATURE_STATUS.md).

The repository's GitHub workflow currently runs only when manually dispatched.
It tests, builds, and uploads Actions artifacts; pushing does not trigger it and
it does not publish a GitHub Release.
