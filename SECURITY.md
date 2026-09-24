# Security

PWR runs a language model's output as commands against a repository on your
machine. That is the whole point of it, and it is also the risk, so this
document says plainly what the boundary is and where it ends.

**It is a public alpha. Do not point it at anything you cannot afford to lose,
and do not run it unattended on a repository you did not write.**

## Current policy — 2026-09-23

**Two permission modes, per workspace.** A conversation runs in **Ask**, the
default, or **Auto**, chosen in the app (`permission_mode` in the workspace's
`.poorai/chat-config.json`, or `_poorai/approvals` over the protocol):

- **Ask** asks before changing dependencies (manifests, lockfiles and the
  installed packages themselves), reaching the network, installing
  toolchains, rewriting Git history and publishing. Running local services and
  adopting a proposed check inside the workspace are granted. A configuration
  saved before the modes existed is read as Ask.
- **Auto** grants every approval category without asking. It lifts the
  question, not the boundary: writes stay confined to the workspace and the
  policy's own refusals still hold.

Scripted runs (`pwr run`) grant only what `--approve` names.

**Confinement.** Commands run under macOS's Seatbelt profile, which confines
writes to the workspace and denies reading the rest of the home. **macOS is
the only platform with a sandbox adapter.** Elsewhere -- or wherever the
profile cannot be expressed -- a command is **refused** unless
`POORAI_ALLOW_UNCONFINED=1` is set, and then it runs with your full rights and
is recorded as `sandboxed: false`; the app shows "commands not sandboxed" when
that is the case. Until 2026-09-23 such commands ran unconfined silently.

## When confinement is dropped — history

For a short time on 2026-09-13, a command whose Seatbelt profile could not be
applied was re-run without the sandbox when it printed the sandbox's own
error. The fallback was **removed the same day** (`b5bd5628`): a sandboxed
command controls its own stderr, so stderr could not be the evidence that won
it an unconfined retry. The adversarial test
`a_command_cannot_print_the_sandbox_error_to_escape_confinement` holds it.

## What the boundary actually is

On macOS a tool runs under a seatbelt profile:

- **Writes** are confined to the workspace root. A write outside it is refused,
  and so is a move whose destination is outside it.
- **Reads** are denied outside the workspace, with an allowlist for the system
  paths a process needs to start and the toolchain directories the repository's
  own build systems name. Documents, mail, browser profiles and other
  repositories are neither readable nor listable.
- **The network** is denied unless a grant names it.
- **Credentials** — `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.netrc`, `~/.kube`,
  `Library/Keychains` and others — are denied to every run, grant or no grant.
- **`HOME` and `TMPDIR`** point inside the workspace, so a package manager's
  caches stay inside the boundary rather than widening it.

Every tool attempt is recorded in a hash-chained log, denied as well as
allowed, and `pwr report --format jsonl <run>` will tell you whether that
chain still holds.

## Where the boundary ends

These are not oversights; they are the known limits of the current design.

- **`--provision` grants an arbitrary executable and the network together**,
  because neither alone can install a toolchain. Under that grant a command can
  still read the system and toolchain paths. The flag's own help says to use it
  only for work you are willing to watch. Running provisioning in a separate
  process or VM is unbuilt.
- **Linux and Windows have no sandbox adapter.** On those platforms
  `SandboxPolicy::Required` refuses to run rather than running unconfined, and
  `Preferred` runs unconfined and records that it did. Check `sandboxed` on any
  result before trusting it.
- **A repository is untrusted input.** A file that instructs the agent to fetch
  and run something is prose; the command still has to pass policy. The frozen
  corpus contains such a file deliberately.
- **The model backend is trusted to be local.** A non-loopback endpoint is
  refused unless `--allow-remote-endpoint` is given, because a prompt carries
  repository excerpts with it.

## Reporting something

Open an issue describing what you observed and how to reproduce it. If you
believe you have found a way for a tool to write, read or reach outside the
boundary described above, please say so in the issue title so it can be looked
at first. There is no embargo process on an alpha.
