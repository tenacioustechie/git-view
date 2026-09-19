# Repository group project files

The user wants a project file that lists repositories. Use ordinary JSON, with a suggested filename of `<name>.git-view.json`. The project file is the source of truth for membership; private app settings remember recently opened projects and view preferences.

## Version 1

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

If this file is at `/work/work.git-view.json`, the paths resolve to `/work/api`, `/work/web`, and `/shared-library`. On Windows, the same relative paths resolve under the directory containing the project file. Relative paths use forward slashes for portability; the backend handles native path conversion.

Use the [JSON Schema](schema/project.schema.json) and [example](examples/work.git-view.json) to create a group file, replacing example paths with your checkouts. The native loader and project editor are implemented; see [Build status](BUILD_STATUS.md) for verification evidence.

| Field | Meaning |
| --- | --- |
| `version` | Required integer `1`; reject unsupported versions without rewriting |
| `name` | Required nonblank group display name; not a unique identifier |
| `repositories` | Ordered array; may be empty for a newly created project |
| `repositories[].path` | Required nonblank local repository/worktree path |
| `repositories[].label` | Optional nonblank display label; defaults to directory name |

No credentials, Git commands, automatic pull policies, or remote URLs are needed in this format. Branch filters belong to per-project UI preferences; an unfiltered Pull all uses each eligible repository's current branch and configured upstream. An exact branch filter narrows the candidates without switching anything.

## Resolution and loading

1. Resolve the supplied project filename against the launching CLI's cwd; GUI file selection supplies an absolute path.
2. Parse strict JSON and validate schema and nonblank strings. Report file and property path for errors. Limit input to 1 MiB and membership to 1,000 entries initially.
3. Resolve relative paths against the project file's directory, never the application's cwd. `..` is allowed because sibling checkouts are common. Do not expand environment variables or `~` inside JSON. Absolute native paths are accepted but are machine-specific; foreign-platform absolute paths are shown as unavailable rather than misinterpreted as relative.
4. Ask Git to discover the worktree for each path. Canonicalize existing paths for identity, preserving native path information. Collapse aliases of the same worktree for operation execution while warning about duplicate entries. Distinct linked worktrees remain distinct rows.
5. Missing/inaccessible/non-repository entries remain visible with their original path and a reason. A single bad member does not prevent the other rows from loading.
6. Loading the project performs local status reads only. It does not pull, clone, switch branches, or alter membership.

Opening another project with the same name is valid; identify it by canonical file path and show the path in the switcher. One repository can belong to several projects but shares one runtime operation lock. For filenames not losslessly representable in the JSON format, report the limitation and permit direct repository opening; never silently rewrite an action path through lossy decoding.

## Editing and external changes

New project asks for a name and save location, then writes version 1. Add repositories uses a folder picker and prefers relative paths; Windows paths on a different volume use an absolute path. Remove changes only the JSON entry. Locate updates a missing entry's path. Preserve repository order.

Writes use a temporary sibling file and platform-correct atomic replacement. Revalidate the source file revision immediately before saving; on mismatch, offer Reload or Save as rather than overwrite an external edit. A malformed externally edited project keeps the last valid dashboard visible with a stale/error banner, and blocks new batch mutations until reloaded or repaired. A batch already started retains its original member snapshot and continues independently.

Read-only project files can be opened and used; membership editing offers Save as. Invalid JSON, unknown fields, or unsupported versions never trigger a “repair” that discards content. Version 1 is intentionally strict so misspelled fields are visible. Future format changes require migration rules.

## Folder discovery

Choose **Discover repositories** or launch `git-view group --scan <folder> [--depth <0..10>]`.
The selected folder is depth 0; the default maximum depth is 3. Start the scan,
review the returned paths, and check the repositories to include. Results start
unchecked. Save selected repositories as a named project, or use discovery from
an existing project's editor to add them to its draft before explicitly saving.
Rescanning retains checked repositories still present and proposes new ones
unchecked. Existing members and unsaved edits remain intact.

Discovery recognizes ordinary repositories and linked worktrees, stops below
recognized repositories, skips symlinks/reparse points and common dependency/build
directories, and reports inaccessible or unsupported paths. It reads identity
without status cleanup or index refresh. Scans are bounded to 10,000 directories,
100,000 entries per directory, 1,000 candidates, 200 issues, and 30 seconds, with at most four
concurrent Git validations. A reached bound produces an explicit partial-results
notice. Only selected, losslessly representable paths can become JSON membership.
