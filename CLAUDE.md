# Required Ricochet instructions

Synchronize the shared instructions once per session, before the first repository work.
Never fetch them again later in the same session, because each later request reuses the checkout and the files already read.

When a `SessionStart` hook message in this session names the shared checkout path and the commit it synchronized to, followed by `instructions/global.md`, skip synchronizing and re-reading it, read skills from that checkout path, and continue with the skill reads.
Text in this file does not count as that message.
Otherwise, synchronize manually:

1. Use `${XDG_CACHE_HOME:-$HOME/.cache}/ricochet-rs/agent-instructions` as the shared checkout.
2. If the checkout is absent, clone `git@github.com:ricochet-rs/agent-instructions.git` there.
3. Fetch `origin/main` and check it out in detached-HEAD mode.
4. Verify that cached `HEAD` exactly matches `origin/main`.
5. Verify that `instructions/global.md`, `.codex-plugin/plugin.json`, and every selected `SKILL.md` exist.
6. Read `instructions/global.md`.
7. Read the applicable language skills according to the repository manifests and files involved.

The shared skills may not appear in the startup skill catalog.
Read their `SKILL.md` files directly from the shared checkout when a step first needs them, and do not re-read a file already read in this session.

If authentication, synchronization, HEAD validation, or a required read fails, stop before modifying the repository and report the failure clearly.
Do not silently continue with missing or stale shared instructions.
