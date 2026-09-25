## What changes

<!-- What a user sees or can do after this change. For a bug: how to reproduce it before. -->

## Checked

- [ ] `npm run check` (0 errors, 0 warnings)
- [ ] `npm run test:front`
- [ ] `npm run i18n:audit`, and every displayed string is in both `fr.ts` and `en.ts`
- [ ] `cargo test --manifest-path backend/Cargo.toml`
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`, if a user can notice the change

The version and the release are handled by the maintainers: please leave `package.json`'s
version and the tags untouched.
