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

The [JSON Schema](schema/project.schema.json) and [example](examples/work.git-view.json) are documentation artifacts available now. The loader/editor is planned implementation.

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

M1 may document hand-editing JSON as a fallback if project creation/editing UI is unfinished, but must still open, validate, resolve, and display a real project file. That fallback must be identified in the morning report rather than claiming full project editing support.
