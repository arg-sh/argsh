# Contributing

Thank you for considering contributing to argsh! This document will outline how to submit changes to this repository and which conventions to follow. If you are ever in doubt about anything we encourage you to reach out either by submitting an issue here or reaching out [via Discord](https://discord.gg/VsQpUQX3Zr).

If you're contributing to our documentation, make sure to also check out the [contribution guidelines on our documentation website](https://arg.sh/contribution-guidelines).

### Important
Our core maintainers prioritize pull requests (PRs) from within our organization. External contributions are regularly triaged, but not at any fixed cadence. It varies depending on how busy the maintainers are. This is applicable to all types of PRs, so we kindly ask for your patience.

If you, as a community contributor, wish to work on more extensive features, please reach out to CODEOWNERS instead of directly submitting a PR with all the changes. This approach saves us both time, especially if the PR is not accepted (which will be the case if it does not align with our roadmap), and helps us effectively review and evaluate your contribution if it is accepted.

## Prerequisites

- **You're familiar with GitHub Issues and Pull Requests**
- **You've read the [docs](https://arg.sh).**
- **You've used argsh**

## Issues before PRs

1. Before you start working on a change please make sure that there is an issue for what you will be working on. You can either find and [existing issue](https://github.com/arg-sh/argsh/issues) or [open a new issue](https://github.com/arg-sh/argsh/issues/new) if none exists. Doing this makes sure that others can contribute with thoughts or suggest alternatives, ultimately making sure that we only add changes that make

2. When you are ready to start working on a change you should first [fork the argsh repo](https://help.github.com/en/github/getting-started-with-github/fork-a-repo) and [branch out](https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-and-deleting-branches-within-your-repository) from the `main` branch.
3. Make your changes.
4. [Open a pull request towards the develop branch in the argsh repo](https://help.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request-from-a-fork). Within a couple of days a argsh team member will review, comment and eventually approve your PR.

## Workflow

### Commits

Strive towards keeping your commits small and isolated - this helps the reviewer understand what is going on and makes it easier to process your requests.

### Pull Requests

If your changes should result in a new version of argsh, you will need to generate a **changelog**. Follow [this guide](https://github.com/changesets/changesets/blob/main/docs/adding-a-changeset.md) on how to generate a changeset.

Finally, submit your branch as a pull request. Your pull request should be opened against the `main` branch in the main argsh repo.

In your PR's description you should follow the structure:

- **What** - what changes are in this PR
- **Why** - why are these changes relevant
- **How** - how have the changes been implemented
- **Testing** - how has the changes been tested or how can the reviewer test the feature

We highly encourage that you do a self-review prior to requesting a review. To do a self review click the review button in the top right corner, go through your code and annotate your changes. This makes it easier for the reviewer to process your PR.

#### Merge Style

All pull requests are squashed and merged.

### Testing

All PRs should include tests for the changes that are included. We have two types of tests that must be written:

- **Unit tests** found under `libraries/*.bats`

Check out our [local development documentation](https://arg.sh/usage/local-development) to learn how to test your changes both in the argsh repository and local.

### Documentation

- We generally encourage to document your changes through comments in your code.
- If you alter user-facing behaviour you must provide documentation for such changes.
- All Bash functions should be documented using [SHDoc](https://github.com/reconquest/shdoc/tree/master)

### Release

The argsh team will regularly create releases from the develop branch.