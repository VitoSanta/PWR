> **Historical record, written 2026-09-12 or earlier.** The body describes the revision it was written against and is kept as evidence of it; its present tense is that revision's. It is **not** a description of the current system. Since then, among other changes: Ollama and LM Studio were removed (2026-09-19) and PWR runs models on its own MLX engine, with llama.cpp in progress; the conversation became the product loop; the desktop app is Tauri 2 + Angular; conversations run in an Ask or Auto permission mode. **For the current state read** the [README](../README.md) (status), the roadmap's latest update ([`roadmap.md`](roadmap.md)), the backlog's *At a glance* ([`backlog.md`](backlog.md)), [`current-cli.md`](current-cli.md), [`pwr-serve.md`](pwr-serve.md) and [SECURITY.md](../SECURITY.md). Dated sections added after 2026-09-12 describe their own date.

# Verification and Recovery

Verification is deterministic whenever the repository permits it: formatter/linter, targeted tests, build/typecheck, then broader suite according to policy. Baselines establish pre-existing failures. Each check records command policy ID, environment fingerprint, exit code, bounded logs, timing, and artifact hash.

Recovery taxonomy: compilation/type error, test assertion, tool/environment failure, context/provider failure, policy denial, and non-determinism. For code failures, retrieve diagnostic locations, make one hypothesis-linked correction, rerun the narrow check, then escalation check. For infrastructure failures, do not modify code until the failure is classified. Default budgets: 3 edit-verify cycles and 1 context-tier retry; make configuration explicit.

## Language coverage

"Appropriate to the repository" means any repository, so check discovery is a registry keyed on marker files rather than a chain of conditions: adding a language is adding a row, and the set of repositories PWR works in is not decided in this file.

Checks are resolved from three sources, ordered by how directly each speaks for the repository.

**An explicit declaration** at `.poorai/checks.json` is the repository saying how it is verified, and wins.

**Continuous integration configuration** is the repository *doing* it: not a guess about the project but the commands its authors run to check it, and it exists for languages and frameworks nobody here has heard of. GitHub, GitLab, CircleCI, Azure, Jenkins, Travis, Bitbucket and Drone are read as text rather than parsed per vendor, since a parser per vendor would be the same closed list one level down.

Steps are excluded on **effect** rather than vocabulary: anything that deploys, publishes, pushes or reaches the network is not a check whatever it is called, and a step that chains or redirects is a script whose first word would not mean what the file says. Words that usually mark verification are a **preference** for ranking, never a filter — `rebar3 ct` and `zig build test` are both verification, and a list of recognised words closes the world exactly as a list of recognised languages does.

**A marker-file registry** is the fast path where neither exists: cargo, go, maven, gradle, dotnet, swift, flutter, mix, poetry, pytest, bundler, composer, make, ctest, and npm where a test script exists. This is PWR guessing from a file name, which is why it ranks last.

A repository matching none of the three yields no checks, and the run says it verified nothing rather than completing as though it had passed.

**What is still closed.** A project with no declaration, no CI configuration and no recognised marker cannot be verified, and there is no mechanism yet for the agent to read a README or a Makefile and work out how the project is checked. That is the remaining step toward genuinely any framework, and it carries a question this design has not answered: a check the agent proposes for itself is a command nobody authorised, so it would need the approval path rather than being run because it looked plausible.

The command allowlist is derived from the same detection. A fixed list decides in advance which languages the agent can work in, and a project whose own toolchain is denied cannot be verified at all.

## Recovery, as implemented

**A failing check is reproduced before it is classified.** `classify_with_reproduction` re-runs the failed command and compares the two results, which is what separates a genuine assertion from a flake and from an environment failure. It was written, tested, and until 2026-09-03 never called: the production branch assigned `FailureClass::Assertion` to every failure it saw. The taxonomy above was real in the type system and absent from the loop, so an environment failure authorised an edit — the exact case the paragraph above forbids.

**The budgets come from the execution profile.** Recovery previously constructed `RecoveryBudget::default()` at the call site and passed the run's total action count as the edit attempt count, so reads and searches consumed the edit-verify budget and the profile's declared numbers bound nothing. `ExecutionProfile.budgets` is now parsed as a typed `ExecutionBudgets` and it is what both the loop and recovery spend.

**A context retry steps to a measured tier.** `RetryContextTier` selects the next calibration point below the current context. Where none is lower, recovery stops rather than choosing a smaller number by arithmetic.

**The diagnostics reach the deployment.** Each failing check's command, exit code, both streams, duration, artifact hash and truncation flags are carried into the next turn, rather than the bare fact that something failed.

## No verifier is a failure

A repository matching none of the three sources yields no checks, and a run over it **cannot complete**. It previously recorded `task.complete`, returned success and exited 0, with `verifiable: false` noted beside it; a caller reading the exit code was told a task had succeeded that nothing had checked. The run now refuses the completion and persists `task.failed` naming the absent verifier.

This is deliberately strict, and it costs something real: the two toolchain-provisioning runs recorded in the roadmap built correct programs in workspaces created from nothing, which by construction declare no checks. Under this rule they are failures. That is the honest reading — the deployment verified its own work against a specification and the harness cannot confirm it — and the way out is a verifier the run can be given or asked to approve, not a completion accepted on the deployment's word.

**Closed as of 2026-09-03: a verifier a person adopts.** `propose_verifier` offers a command and runs nothing; the question names the command and the reason. Approved, it becomes a check the run is judged against and its executable joins the allowlist. Refused, the workspace still has no verifier and completion is still refused. It is adopted by the loop rather than by the tool, because a check outlives the action that proposed it, and the adoption is recorded as `verifier.adopted` rather than inferred from the run having succeeded.

Granting `--approve verifier-proposal` in advance lets an unattended run adopt one; without it, a proposal with nobody attached is refused, which is the same rule every other approval follows. Diagnostics are bounded text rather than typed locations, so recovery aims at a paragraph rather than at a file and a line.

### A page is a workspace with one check — 2026-09-06

The strict rule above assumed that a workspace with no toolchain has nothing
deterministic to check. The CV-portfolio campaign showed one kind that does. The
run wrote `index.html`, `style.css` and `main.js`, and the HTML asked for
`script.js`. Everything generic said it was sound: seven sections present, the
selected terms from the source all found, `node --check` green on the script, no
duplicate ids, no broken internal anchors. Rendered in a browser afterwards, the
delivered page showed its header and then 6,500 px of nothing — 27 blocks left
at `opacity: 0` by a stylesheet whose script never loaded — and on a phone it had
no navigation either, because the menu is moved off-screen by CSS and only that
script brings it back. Zero of 27 blocks visible after a full scroll; 27 of 27
with the file renamed and nothing else changed.

So `pwr:web-assets` is discovered when a workspace has markup and no other
source speaks for it: every local thing a page tells the browser to load must
exist. It is ranked below the declaration, the CI configuration and the build
registry, because a project that states how it verifies itself has said
something stronger. It runs in-process — no executable, no allowlist entry, and
its record says `sandboxed: false` rather than claiming a confinement it never
needed. Against the campaign's own delivered workspace it reports
`index.html:588: src="script.js" no such file in the workspace` in 5 ms, which
is a located diagnostic in the sense of the section below.

A reference that resolves to a zero-byte file is reported the same way as one
that resolves to nothing, and for the same reason: the page loads nothing either
way. That was found the hard way — the check stayed green over the emptied
`js/main.js` described in [tool-runtime.md](tool-runtime.md), because existing
had been the whole test.

What it is not: a browser. It says nothing about how a page looks, whether an
animation runs, or whether the script it found does anything useful. It only
refuses to call a page finished while it points at a file that is not there, and
the runs of 2026-09-06/07 are the evidence that this is not enough: both
deployments delivered pages whose HTML, CSS and JavaScript disagreed about the
same class or attribute name, and this check was green over every one of them.
Stylesheet `url()` references and a script's own fetches are out of scope for
the same reason the build registry is a table of markers: a check that is
sometimes wrong about what a page needs is worse than one that is always right
about part of it.

## Located diagnostics — 2026-09-03

Check output reached the deployment as bounded prose, so recovery aimed at a paragraph and finding the file and line was work the model paid actions for -- mechanical work, which is the harness's job. rustc, gcc-style and Python traceback shapes are read into a path, a line and a column and travel with the failing check. Deliberately shallow, and shallow safely: a line that does not clearly carry a path and a position is not guessed at, because a wrong location is worse than none -- it sends the agent to edit a file that is fine. The prose is still carried, since a diagnostic the parser did not recognise must not disappear because of it.

## Acceptance and explicit existing failures — 2026-09-06

Normal runs require at least one non-exempt deterministic check, and **all
non-exempt checks must pass** at completion. An unchanged red baseline alone
cannot certify a repair. The initial baseline now includes narrow checks,
completion-time broad checks and declared exceptions; broad regressions are
compared against the original workspace. Checks adopted during the run have
no initial baseline and must pass.

An owner can declare a separately quarantined failure in `.poorai/checks.json`:

```json
{
  "checks": [{"executable": "cargo", "args": ["test", "--test", "acceptance"]}],
  "known_failures": [{"executable": "cargo", "args": ["test", "--test", "quarantined"]}]
}
```

Both commands still run before and after. A known failure can become green;
if it remains red, its exit code and both output streams must be identical,
untruncated and unredacted. Exceptions are loaded before the loop from the
protected configuration; the model cannot excuse a red result in its rationale.
All-exempt checks do not constitute acceptance. Invalid exception configuration
is an invocation error.

This comparison is conservative: changed timing printed in a failing test's
output may block completion. Conversely, identical aggregate diagnostics cannot
prove that every individual test kept its status. Isolate known failures into
separate commands; do not quarantine the entire acceptance suite. Passing checks
still establishes only what those checks cover, not the full natural-language
request.

The evaluation runner explicitly selects baseline preservation for read-only
`RepositoryQuestion` tasks, whose answer and untouched tree are scored outside
the loop. It is not available as a model action or a normal-run waiver.
`verification.contract` records this mode and the initial commands/exceptions;
the returned completion outcome distinguishes `acceptance_passed` from
`preserve_baseline_only`.

### Read-only correction found by the live pilot — 2026-09-06

The first v5 pilot exposed two defects in the diagnostic exception above. The
repair system prompt still demanded green tests, and red Cargo output changed
between invocations (including timing), causing a correct unchanged answer to
be refused. The deployments' recorded rationales explicitly identified the
conflict; some then edited despite the task's prohibition.

Read-only evaluation now supplies a separate system prompt. Completion requires
unchanged indexed source contents (path/content hashes, excluding mtimes) and
preserved check exit statuses; newly failing or unfinished checks still reject
completion. `verification.read_only` records both conditions. The independent
evaluator still scores the answer and the final workspace snapshot. This rule
is specific to diagnostic answers; normal repairs and known-failure exceptions
retain strict acceptance and diagnostic comparison. Tests vary red log text
without changing source and separately attempt a source edit.
