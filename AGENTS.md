# Repository instructions

These instructions apply throughout this repository, including future feature,
bug-fix, test, and documentation work.

## Documentation is part of a user-facing change

- Read `docs/USER_GUIDE.md` and `docs/FEATURE_STATUS.md` before changing user-facing
  behavior. Use `docs/PRODUCT.md` for the broader plan and `docs/BUILD_STATUS.md`
  for existing verification evidence; planned behavior is not implemented behavior.
- Update `docs/USER_GUIDE.md` in the same change whenever adding, removing, or
  changing a screen, action, label used in instructions, shortcut, CLI option,
  project-file behavior, prerequisite, limit, error/recovery workflow, or package
  launch/install behavior. Describe actual controls and executable steps.
- Update `docs/FEATURE_STATUS.md` when scope or completion changes. Separate
  implemented features, future work, intentionally excluded scope, and unverified
  acceptance/platform checks. Do not equate the runbook's M2/M3 workflows with
  completion of every item in the product roadmap.
- Update the guide/inventory review date when reviewing their behavior coverage.
  Identify the actual source baseline when known; never invent a tested revision.
- Record meaningful implementation gates, test commands/results, platforms,
  limitations, and artifact revisions in `docs/BUILD_STATUS.md`. Preserve older
  dated evidence. Do not present an old test result as validation of new code.
- Keep `README.md` entry points and affected specifications (such as
  `docs/PROJECT_FILES.md`, `docs/CONTRACTS.md`, and recovery/acceptance docs)
  consistent. Update guides for changed CI/release behavior too.
- Before completion, compare affected guide steps with the final controls/CLI,
  check local links and `git diff --check`, and run checks appropriate to the code
  change. Documentation-only edits do not require repeating native builds.
- If no user-facing documentation is affected, explain why in the handoff or PR.
  This is a maintenance requirement, not an automatic documentation generator.

Preserve unrelated edits. Follow the ownership boundaries in `docs/OVERNIGHT.md`
when working with a team. Use disposable repositories for mutation tests. Do not
push or publish releases unless the user explicitly requests it.
