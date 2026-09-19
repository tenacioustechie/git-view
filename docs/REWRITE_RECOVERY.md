# Combine commits recovery design

M3 is separate from squash-merging a branch. Only an attached branch with a clean
index/worktree and at least two consecutive linear tip commits is eligible.
The oldest selected commit must have a parent; root and merge commits are rejected.
The preview lists the exact selected commits, original tip, parent, replacement
message, and history-rewrite warning. Remote-tracking state cannot prove publication.

The implementation must persist a versioned, bounded journal in the worktree Git
directory and create a unique backup ref before preparing a replacement. Journal
writes use an atomic sibling-file replacement and flush. A temporary app-owned
detached worktree starts at the suffix parent, receives the original tip tree, and
runs a normal commit with hooks/signing. The user's branch, index, and working
files remain unchanged during preparation. Effective identity/signing/hook settings
must be preserved; unsupported context differences are rejected, not bypassed.

Before installation, verify that the replacement has exactly the suffix parent
and the original tree. Anchor the replacement with a separate ref, and persist its
OID and the installing phase. Under the application common-directory lock, recheck
branch/HEAD/config and clean state. Use Git-managed reference transactions to guard
HEAD and compare-and-swap the branch. A disposable probe established that verifying
HEAD and updating its referent in one transaction produces a duplicate-update
error; a separate bounded HEAD guard plus a transaction from the detached worktree
avoids that error. No manually created/deleted Git lockfiles are permitted.

Recovery compares actual refs with the journal. If the original branch still points
at the original tip, no branch restoration is needed. If it points exactly at the
verified replacement and the backup remains intact, a guarded ref-only CAS restores
the original tip. Since both tips have the same tree, staged/unstaged/untracked
changes can remain byte-for-byte unchanged. A later commit, branch switch, changed
backup, corrupt journal, or unexpected phase/ref state requires manual review;
never blindly reset or discard files. Keep backup and replacement refs for recovery.

Tests must cover successful tree/parent/count/message preservation, invalid suffixes,
hook and signing rejection, hooks changing the candidate tree, interrupted phases,
backup tampering, branch/HEAD races, and recovery with later file edits and commits.
The UI is enabled only after these tests and native workflow acceptance pass.

Primary references: [Git reference transactions](https://git-scm.com/docs/git-update-ref),
[worktrees](https://git-scm.com/docs/git-worktree), and [hooks](https://git-scm.com/docs/githooks).
