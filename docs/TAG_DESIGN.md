# Create tag at selected commit — design

Prepared 20 October 2026 against source and documentation checkpoint `33a937f`.
Status: **implemented in the 0.2.0 source; local and cross-platform package
acceptance remains outstanding**. [Feature status](FEATURE_STATUS.md) is the
implementation inventory, [User guide](USER_GUIDE.md) describes the delivered
controls, and [Build status](BUILD_STATUS.md) records current evidence.

## Outcome

Let someone select any commit visible in History and create a named local tag
that points to that exact commit. The target must remain clear through entry,
preview, confirmation, execution, and refresh. Selecting a historical commit is
the normal workflow; the action must not silently substitute HEAD or the current
branch tip.

The recommended first slice creates one annotated tag in the local repository.
It does not push the tag, replace or delete an existing tag, move a tag, create
a lightweight tag, or add a general tag-management screen. Those operations have
different publication and recovery consequences and should be designed separately.

## Current implementation findings

- `list_refs` already reads `refs/tags/`, peels annotated tags to their target
  object, and returns tag labels and commit IDs. History renders those tags as
  commit badges and offers tag-specific history scopes.
- Selecting a History row already retains the full commit ID, highlights the row,
  and loads a backend-issued commit-detail snapshot. That snapshot binds the
  repository to the exact selected commit and is suitable as the mutation token.
- **Create branch** can start at the selected commit. Tag creation now has a
  separate selected-commit form, shared action/preview contracts, backend
  preflight, guarded execution, postcondition checks, and activity metadata.
- Single-repository and group push deliberately suppress automatic tag following.
  Creating a local tag therefore cannot accidentally publish it through an
  existing Git View push action.
- A disposable Git 2.51.2 probe created an annotated tag on the first of two
  commits while HEAD was on the second commit and the worktree was dirty. The tag
  peeled to the first commit, the working file remained dirty and unchanged, and
  a duplicate-name attempt failed without moving the existing tag.

## Delivered product decisions

These decisions define the delivered first slice. They do not define broader
tag-management requirements.

1. **Target only an explicitly selected commit.** Show **Create tag…** in the
   selected commit inspector after commit details load. Do not offer an implicit
   “tag HEAD” action. A user can select the current tip when that is the intended
   target.
2. **Create an annotated local tag.** Require a nonblank annotation message and
   use the normal Git tagger identity. Annotated tags carry the tagger, creation
   time, and message and are the form Git recommends for releases.
3. **Respect configured tag signing.** Read the effective `tag.gpgSign` state for
   the preview. Normal configured signing, identity, and signing-program behavior
   remains in effect. A signing or identity failure creates no successful result
   and keeps the form available for correction or external configuration.
4. **Never replace an existing tag.** Do not use `--force`. An exact local name
   collision blocks the plan; a tag created externally after preview blocks
   execution. Retagging a published name can give different users different
   objects and needs a separately designed workflow.
5. **Keep publication separate.** The preview and activity summary identify the
   local ref and explain that no remote is changed. Explicit push-tag review, remote tag
   deletion, and hosted-release integration are later slices.
6. **Allow ordinary dirty files.** Creating a tag changes refs and possibly adds
   a tag object; it does not read from or change the index or worktree. Staged,
   unstaged, untracked, and submodule changes do not block it. An active merge,
   cherry-pick, rebase, revert, squash review, or app mutation does block it so
   the repository context and resulting activity stay unambiguous.
7. **Support commits of any parent shape.** A root, linear, or merge commit is a
   valid target. The operation records the selected commit object and does not
   traverse or rewrite its ancestry.

## User workflow

1. Open **History** and select the intended commit in the DAG or search results.
   The existing selected-row highlight remains visible.
2. In **Commit details**, choose **Create tag…**. The form displays the commit
   subject and full ID as a read-only target. It contains **Tag name** and
   **Annotation message** fields.
3. Enter a short tag name such as `v0.2.0` or `release/0.2`. The UI accepts a
   short name only. It rejects a leading `-`, a `refs/` prefix, NUL or line-break
   characters, and values Git rejects for `refs/tags/<name>`.
4. Choose **Preview tag**. Changing the selected commit while the form or preview
   is open closes the form and discards that target-specific draft. A delayed
   preview for an older selection is ignored.
5. The confirmation names the tag, shows the full target commit ID and subject,
   identifies it as an annotated tag, states whether current Git configuration
   requires signing, and says that the tag remains local. Confirm with **Create
   local tag** or cancel and return focus to the form.
6. On success, refresh refs/history without changing the selected commit. The new
   badge appears on that row and the name appears under **Tags** in History scope.
   The activity entry records `createTag`, the local ref, and target commit.

The name and message draft belongs to the selected commit and current repository.
It is cleared after success or target change. A failed preview or execution keeps
the fields so the user can correct the name, identity, or signing configuration.

## Eligibility and validation

- Require a backend-issued commit-detail snapshot whose repository and stored
  commit ID exactly match `targetOid`. Do not accept a renderer-supplied revision
  expression, ref name, abbreviated ID, or object that was not selected through
  a current snapshot.
- Re-resolve the full target ID with replacement refs disabled and require a
  commit object. The target may cease to be reachable from a branch after preview;
  if the exact object still exists, creating the tag is useful and remains safe.
- Trim no tag-name characters silently. Bound the UTF-8 name to 255 bytes, reject
  blank names, leading `-`, and `refs/`, then run `git check-ref-format` against
  the constructed full `refs/tags/<name>`. Slash-separated names are allowed.
- Require a nonblank annotation of at most 64 KiB and reject NUL bytes. Preserve
  intentional internal line breaks. Pass it on standard input instead of placing
  it in process arguments or a temporary file.
- Resolve the tagger identity during planning. Show an actionable missing-identity
  error before confirmation. Record whether effective Git configuration requires
  signing and bind the plan to the configuration fingerprint.
- Require `gitState: idle` and no active Git View operation for this repository.
  Working-tree dirtiness and detached HEAD do not block an exact historical
  target. An unborn repository has no selectable commit and therefore no entry
  point.
- Require `refs/tags/<name>` to be absent during planning and execution. Inspect
  both loose and packed refs through Git, not filesystem paths. The common Git
  directory lock serializes Git View mutations across linked worktrees; Git's ref
  transaction remains authoritative against external writers.
- Plans expire after the existing five-minute interval. Execution rechecks the
  snapshot, config, operation state, exact target object, and missing destination
  ref under the repository lock.

## Contracts

Extend the shared TypeScript and Rust `RepositoryAction` union with:

```ts
{
  kind: 'createTag';
  commitSnapshotId: SnapshotId;
  targetOid: Oid;
  name: string;
  message: string;
}
```

The action intentionally has no `statusSnapshotId`: unrelated working-file
changes should not make a tag preview stale. The commit snapshot and exact OID
bind the selected target. Backend observation still gates active Git operations.

Extend an eligible plan with structured data instead of relying only on summary
text:

```ts
tagPreview?: {
  name: string;
  fullRef: string;
  targetOid: Oid;
  subject: string;
  annotated: true;
  signing: 'configured' | 'unsigned';
}
```

The operation result uses action `createTag`, destination
`refs/tags/<name>`, and a summary containing the exact target ID. No remote name
or remote destination is present.

## Git execution and postcondition

Use an argument array and standard input, structurally:

```text
git --no-replace-objects tag --annotate --file=- -- <name> <full-target-oid>
```

Do not pass `--force`, synthesize a shell command, open an editor, or resolve the
target from HEAD. Preserve effective `tag.gpgSign`: Git may sign the annotated
tag when configured. Existing runner diagnostic limits and timeouts apply.

After a zero exit, verify all of the following before reporting success:

- `refs/tags/<name>` exists as a direct tag ref;
- its object type is `tag`, proving the result is annotated;
- peeling `refs/tags/<name>^{commit}` yields the reviewed target ID; and
- HEAD, index, and worktree observations relevant to the action remain unchanged.

If Git reports an error, re-read the destination ref. A newly present matching
tag can be reported as completed only when the backend can prove this invocation
created it; otherwise report an uncertain outcome and ask the user to inspect the
tag. A present tag with a different target is never success.

## Failure and recovery behavior

- **Invalid or duplicate name:** keep the form, identify the field, and make no
  mutation. Never suggest force replacement in the first slice.
- **Selection expired:** refresh commit details and require a new preview. Keep
  the typed fields only while the same repository and target remain selected.
- **Commit unavailable:** report that the selected object disappeared and require
  a history refresh. Do not fall back to HEAD.
- **Identity/signing failure:** show bounded Git diagnostics and explain that no
  local tag was confirmed. The user can fix Git configuration and preview again.
- **Ref lock or reference hook failure:** refresh tag refs. Report an uncertain
  outcome when the command could have updated the ref before the failure became
  visible.
- **App restart:** there is no durable multi-step Git state to recover. Refreshing
  refs reveals whether the tag exists. Form drafts are session-only.

## Deliberately deferred

- Lightweight tags or a tag-type selector.
- A **Sign tag** checkbox, signing-key selector, signature verification UI, or
  overriding `tag.gpgSign`.
- Push one tag, push all tags, remote selection, remote tag deletion, or automatic
  GitHub release creation.
- Delete, rename, move, or force-update a local tag.
- Tag browsing beyond the existing History scopes and badges, release-note
  generation, semantic-version suggestions, or group tag operations.

These can follow only with explicit destination, publication, and recovery
designs. Existing branch pushes continue using `--no-follow-tags`.

## Implementation notes

The implementation adds `createTag` and structured `tagPreview` contracts, a
focused `tags.rs` engine path and `CreateTag` commit-inspector component. It uses
the existing five-minute plan store, common-directory mutation lock, operation
activity pipeline, stale-render guards, and post-success repository refresh.
Normal Push and group Push retain their existing `--no-follow-tags` policy.

## Verification status and remaining acceptance

The working tree has focused Rust coverage for a slash-separated annotated tag
at a non-HEAD historical commit with dirty files, exact peeled target, unchanged
HEAD/file content, structured preview and activity destination. It also covers
duplicate protection, invalid names, a blank annotation and a snapshot/OID
mismatch. Focused renderer coverage verifies exact action fields, full-target and
local/unsigned preview text, cancel, blocked retry and retained input. Exact
commands and results are in Build status.

The broader gate below remains for final package/platform acceptance; unchecked
cases are not implied by the focused implementation tests.

- Backend fixtures create an annotated tag at a non-HEAD historical commit and
  prove tag object type, annotation, tagger, peeled target, unchanged HEAD/index/
  worktree, and no remote ref. Cover root, linear, and merge commits, detached
  HEAD, dirty files, linked worktrees, SHA-1, and SHA-256 where supported.
- Reject blank/oversized messages; blank, leading-dash, `refs/`-prefixed, invalid,
  duplicate, case-colliding where applicable, and externally raced names. Test
  packed refs and slash-separated valid names.
- Reject foreign/expired/mismatched commit snapshots, arbitrary hidden object IDs,
  missing target objects, changed signing/configuration, active Git operations,
  and duplicate execution of one plan without creating multiple outcomes.
- Exercise missing identity, configured signing success/failure where test tooling
  permits, reference-transaction hook rejection, lock conflicts, timeout, and
  post-command verification failure. Assert actual refs after every failure.
- Renderer tests cover selection pinning, full target identity, target changes,
  keyboard/focus behavior, validation, signing/local-only preview language,
  cancel, retry, success refresh, retained selected-row highlight, and the new tag
  badge/scope entry.
- Native macOS and Windows acceptance selects an older commit, creates the tag
  through real IPC, verifies the Git object/ref and unchanged dirty file, then
  observes the badge. Hosted Rust/frontend checks cover Linux and Intel Mac; any
  unrun native GUI checks remain explicit.
- Run typecheck, lint, frontend tests, Rust formatting/Clippy/tests, existing
  native regression suites, package builds, and `git diff --check`. Record exact
  source revisions and results in Build status before marking the feature done.

## Sources and exploration evidence

- [Git tag documentation](https://git-scm.com/docs/git-tag): exact target syntax,
  annotated/lightweight behavior, signing, duplicate protection, and retagging
  risks.
- [Git check-ref-format documentation](https://git-scm.com/docs/git-check-ref-format):
  tag-reference name validation.
- Local disposable Git 2.51.2 probe on 20 October 2026: historical annotated
  target, dirty-worktree preservation, duplicate rejection, nested valid names,
  and invalid whitespace/`..` names. This is design evidence rather than app
  acceptance.
