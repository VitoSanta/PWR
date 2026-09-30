# Manual pass — what to check in the app after the 2026-09-30 changes

**For `develop` at the commit that adds this file.** Nothing below has been
seen in the running app by the author of the changes: the automated suites pass
(Rust, desktop, sidecar), and this list is what those suites cannot show. Record
what you find in this file, with a screenshot for anything visual, the way
[v0.2.x-mac-verification.md](v0.2.x-mac-verification.md) does.

Run it from a checkout: `cargo build --release`, then in `apps/desktop`
`npx tauri dev` (or `npx tauri build --bundles app`). Use a small model you
already have, and a scratch workspace you can throw away.

## Changed behaviour a person will notice

| Check | Expected | Plan |
|---|---|---|
| Ask it to rewrite a file it has **not read** ("replace `a.txt` with …") | The write is refused with an instruction to read the file first; the file is unchanged. After it reads the file, the same rewrite goes through | W1.1 |
| Edit a file yourself **after** the model read it, then have it rewrite that file | Refused as changed since it was read; your edit survives | W1.1 |
| A turn that edits, in a workspace with a check that fails | The note under the answer is `✗` (never `✓`), naming the failing check. With no checks: `–` and "Independent verification unavailable" | W2.2 |
| A turn that edits, all checks green | `✓`; the turn is **not** called "verified" unless a declared acceptance check passed | W2.1 |
| Ask it to build a small program and finish without running it | It is asked once to run it before it can complete | W2.4 |
| Ask something that needs work, and see if it declares "done" on its first call | It is asked once whether anything was done | W2.4 |
| Goal mode, then Stop mid-run | Stops promptly, including during a long prefill on a large prompt | W5.1 |
| Goal mode that cannot finish | Pauses with a **budget** message naming the limit (actions, refused completions, verification runs, review rounds or minutes), not silently | W1.4 |
| Settings: wiki summaries | **Off by default**; switching on writes them in the background, and a message you send pre-empts one in flight | W5.2 |
| Knowledge card | Opens on a searchable **outline**; the 3D graph is behind an *Experimental* switch | W7.2 |
| Run controls → Full access | Every turn that ran a command unconfined says so; permission questions state what the grant actually opens | W1.9, W7.4 |
| Model Manager: a gated repository | (Not done yet — still a generic failure) | W7.7 |

## Also worth a look

- The turn's end: what it did, files changed, checks, why it stopped (W7.1 is
  partly done — say what is missing).
- Reopen a conversation: the reconstructed summary and each turn's own model.
- Edit a file in a workspace with a `.pwr/protected.json` entry through a
  command the model runs (`sh -c 'echo x > protected-file'`): it must be denied
  in Protected and Standard modes (W1.3; Full access has no sandbox).
- Kill the app during an edit and reopen: the file is whole, never truncated
  (W1.2).

## Results

_(fill in)_
