# Instructions for AI coding agents

- **Never add firmware, vendor binaries, decompiled code, extracted strings dumps or keys** to the repository, not even
  in tests. Describe behaviour in your own words in `docs/`, and write code from the description.
- **Scratch output goes in `target/`** (ignored) or the system's temp folder. Nothing new at the repository root.
- **No personal data**: no names, home paths, device serial numbers, account details or the content of anyone's notes.
- **No agent notes, plans or session logs in the repository**; a finding worth keeping becomes a doc in `docs/`
  (listed in `docs/INDEX.md`).
- **Check `git status` first; commit only your own paths**, named explicitly; never push unless asked.
- **Before calling it done**: `cd impl && ./check.sh --no-device-builds` passes.
