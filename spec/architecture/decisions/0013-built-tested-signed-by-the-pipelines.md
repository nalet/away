# ADR-0013: Built, tested, signed and listed by the project's own pipelines

**Status:** Proposed · 2026-11-14. One pipeline is there, the one that checks the model and publishes this site. The rest is not built.

## Context

Anyone should be able to see that Away is what it says ([Trust it](../../goals/README.md#trust-it)): that an image is the one the project built (`verify-image`), what it holds (`see-image-contents`), that it builds from its source with one command (`build-from-source`), and that its tests run on one's own machine against a real server of each protocol (`run-its-tests`). The code that reads what a machine sends is fuzzed on every change and without pause, and reviewed by someone outside before the first release. A stranger's change is taken in on the strength of tests that run on every change.

One kind of test doesn't fit a pipeline that starts from nothing: a Windows machine in a domain can't be started by a build.

## Decision

1. **Everything is built by pipelines in the repository, on GitHub, from the commit.** The tools they use are fixed to exact versions. One command, written in the README, builds the same on one's own machine.
2. **On every change:**
   - the model's check, and the site;
   - the code's form and the compiler's warnings, which count as failures;
   - the tests of each library, with recorded bytes;
   - the tests against real servers that the pipeline starts itself: a shell, a desktop over VNC, a desktop over RDP on Linux;
   - the web app in the engines of the browsers people use, with the checks for accessibility and for anything loaded from outside ([ADR-0011](./0011-the-web-app.md));
   - a short fuzzing of everything that reads a machine's bytes;
   - the libraries Away is built from: licences, known holes, sources ([ADR-0002](./0002-rust-one-program-one-repository.md)).
3. **Without pause:** fuzzing on a schedule, for hours. What it finds becomes a test.
4. **Before a release, the Windows suite**: every test that needs a Windows machine, with and without a domain, run in a place that has them. The suite is one command that anyone can point at machines of their own. Its report is part of the release.
5. **A release is a tag.** The pipeline builds the image for two kinds of processor, signs the image and the chart with the pipeline's own identity and no key that anyone holds, attaches a record of how each was built and a list of what the image holds with every licence, and publishes them to the project's registry. The documentation says how to check each.
6. **A security hole is reported in private**, at an address the repository names, with a first answer in the time the quality sets.
7. **Nothing in a pipeline needs a secret that a stranger lacks**, except publishing. A fork runs every test but the Windows suite.

## Consequences

- An image can be traced to its commit and to the run that built it, by anyone, without trusting the project's word.
- A change that breaks a protocol against a real server is caught before it is taken in.
- What a release holds of other people's work is a list, with licences, made by the build.
- The Windows suite doesn't run on every change. A break there shows before a release, and not sooner unless whoever has such machines runs it.
- Somebody has to keep a place with Windows machines and a domain for testing. The first release can't be made without one.
- Fuzzing without pause costs machine time for as long as the project lives.

### What this rules out

- **No release built on somebody's own machine**, and none that isn't signed.
- **No library taken in without the checks.**
- **No test that passes only with a secret**, the Windows suite's machines aside.
