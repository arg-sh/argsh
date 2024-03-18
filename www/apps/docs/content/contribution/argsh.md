---
sidebar_label: "Argsh libraries"
sidebar_position: 2
---

# Contribute by improving argsh libraries

In this document, you'll learn how you can contribute to argsh by improving the libraries.

## Overview

Argsh is built on top of a set of libraries that are used to parse command line arguments, validate input, and more. These libraries are written in Bash and are used to build the core functionality of argsh.

## Libraries

The libraries are located in the `libraries` directory in the argsh repository. Each library is a separate file and is named after the functionality it provides.

## How to contribute

If you’re adding a new library or contributing to the codebase, you need to fork the repository, create a new branch, and make all changes necessary in your repository. Then, once you’re done, create a PR in the argsh repository.

### Base Branch

When you make an edit to an existing documentation page or fork the repository to make changes to the documentation, you have to create a new branch.

Documentation contributions always use `develop` as the base branch. Make sure to also open your PR against the `develop` branch.

### Branch Name

Make sure that the branch name starts with `argsh/`. For example, `argsh/fix-services`. Vercel deployed previews are only triggered for branches starting with `argsh/`.

### Pull Request Conventions

When you create a pull request, prefix the title with `argsh:`.

<!-- vale off -->

In the body of the PR, explain clearly what the PR does. If the PR solves an issue, use [closing keywords](https://docs.github.com/en/issues/tracking-your-work-with-issues/linking-a-pull-request-to-an-issue#linking-a-pull-request-to-an-issue-using-a-keyword) with the issue number. For example, “Closes #1333”.

<!-- vale on -->

### Generate minified version

If you make changes to the libraries, you need to generate a minified version of the library. You can do this by running the following command:

```bash
make minify
```

## Testing

All libraries should have tests. Create a new test file (`*.bats`) alongside the library file.

You can run the tests by running the following command:

```bash
make test --argsh
```

### Coverage

We strive to have 100% test coverage for all libraries. When you add a new library or make changes to an existing library, make sure to add tests that cover all functionality.

You can run the coverage report by running the following command:

```bash
make coverage
```

## Documentation

The documentation for the libraries is automatically generated from the source code (`shdoc` comments). Make sure to add comments to your code that explain how the library works and how to use it.

You can generate the documentation by running the following command:

```bash
make generate
```

## Code Style

Use our [style guide](https://arg.sh/styleguide) to make sure your code is consistent with the rest of the codebase.

## Linting

To lint the code, run the following command:

```bash
make lint --argsh
```