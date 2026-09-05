# zCore Updated

## Default build command
To build and run zCore the default command to use is:

```bash
cargo qemu --arch aarch64
```

## PR workflow
After pushing commits to a PR:
1. Wait for CI checks to complete
2. **Always check for code review comments** (CodeRabbit and human reviewers) using `gh api repos/andrewdavidmackenzie/zCore/pulls/<PR>/comments` and `gh pr view <PR> --json reviews`
   (replace `<PR>` with the actual pull request number before running)
3. Address all actionable review comments before moving on to new work
4. Push fixes as new commits, then re-check for new comments

This applies after every push, not just the final one. Do not wait for the user to ask -- proactively check and fix review comments.
