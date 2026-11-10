# web — playground (React + Vite + TS)

Generic UI that calls any method of any service from `web/public/descriptors.binpb` (built by
`npm run gen`). No per-service code: a new proto service appears automatically.

- Verify: `npm run verify` (tsc + oxlint + oxfmt check). Format: `npm run format`.
- Readability limits are in `.oxlintrc.json`. Existing code still has 27 warnings
  (`oxlint --max-warnings 27` in package.json). **Lower that number when you refactor; never raise it.**
- Style: no semicolons, single quotes, width 120 (`.oxfmtrc.json`).
