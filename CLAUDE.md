# zCore Updated

## Default build command
To build and run zCore the default command to use is:

```bash
cargo qemu --arch aarch64
```

## Pre-push checks
**Always run `make pre-push` before pushing commits.** This runs the same
checks as CI (clippy, fmt, builds, boot tests, libc tests, feature
combinations) and catches failures locally before they show up in CI.

```bash
make pre-push
```

If `make pre-push` fails, fix the issue before pushing. Do not push
code that fails pre-push checks.

For faster iteration, use `make pre-push-quick` which runs clippy,
fmt, unit tests, and builds (~3 min) but skips QEMU boot tests.
Run the full `make pre-push` before the final push.

## PR workflow
After pushing commits to a PR:
1. Wait for CI checks to complete
2. **Always check for code review comments** (CodeRabbit and human reviewers) using `gh api repos/andrewdavidmackenzie/zCore/pulls/<PR>/comments` and `gh pr view <PR> --json reviews`
   (replace `<PR>` with the actual pull request number before running)
3. Address all actionable review comments before moving on to new work
4. Push fixes as new commits, then re-check for new comments

This applies after every push, not just the final one. Do not wait for the user to ask -- proactively check and fix review comments.
