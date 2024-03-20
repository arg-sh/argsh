---
description: "How to test your argsh scripts"
---

# How to Test with argsh

In this document, you'll get an overview of how to test your argsh scripts.

## Overview

Testing is an important part of software development. It helps you to ensure that your code works as expected and that it continues to work as expected when you make changes to it.

There are different types of tests, but in this document, we'll focus on unit tests and integration tests.

## Testing Frameworks

There are different testing frameworks for bash. The most popular ones are [bats](https://bats-core.readthedocs.io/en/stable/). This is what argsh is using for testing.

## Writing Tests

Every script should have a corresponding test file. The test file should be in the same directory as the script and should have the same name as the script, but with a `.bats` extension.

## Current State

Argsh is using bats for testing. You can have a look at the [tests](https://github.com/arg-sh/argsh/tree/main/libraries) in the argsh repository to see how it's done.

We plan to provide a more integrated testing experience in the future. This will include a test generator and a test runner.