// The execution trace of a run, as the conversation shows it: phases of work
// that open onto their steps. Everything here is derived from the timeline the
// store already keeps -- one stream of protocol events -- so the phases and
// the steps inside them never disagree about what happened.

import { Entry } from './model';

/** What a tool call was, for counting and for choosing its phase. */
export type ToolCategory =
  | 'file_read'
  | 'search'
  | 'file_edit'
  | 'command'
  | 'verification'
  | 'fetch'
  | 'malformed'
  | 'refused'
  | 'other';

export type PhaseName = 'Inspecting workspace' | 'Planning' | 'Implementing' | 'Verifying' | 'Fixing' | 'Finalizing';

/** A step of a turn as a phase shows it when opened: one entry, or consecutive actions folded together. */
export type Step =
  | { type: 'entry'; key: string; entry: Entry }
  | { type: 'actions'; key: string; entries: Entry[]; last: boolean };

export interface Phase {
  key: string;
  name: PhaseName;
  entries: Entry[];
  startedAt: number;
  endedAt: number;
}

export interface CompactTurn {
  phases: Phase[];
  /** The answer the turn ended with, when it wrote one. */
  result: Entry | null;
}

// Commands that check the work rather than change it. A heuristic on the
// command line: the core does not label a command as a check.
const VERIFYING =
  /(^|[\s/])(test|tests|check|lint|verify|typecheck|type-check|tsc|pytest|vitest|jest|mocha|clippy|mypy|ruff|eslint|flake8|pyright|build)(\b|$)|go\s+(test|vet)|cargo\s+(test|check|clippy|build|nextest)|npm\s+(run\s+)?(test|build|lint|check)|make\s+(test|check)/i;

/** A call the core could not read: the model's form, retried, not a refusal. */
export function isMalformed(entry: Entry): boolean {
  return entry.kind === 'tool' && /did not match its declared schema|could not be read|invalid type|missing field|not valid JSON/i.test(entry.text);
}

export function toolCategory(entry: Entry): ToolCategory {
  if (isMalformed(entry)) return 'malformed';
  if (entry.status === 'failed' && !entry.diff) return 'refused';
  switch (entry.toolKind) {
    case 'read':
      return 'file_read';
    case 'search':
      return 'search';
    case 'edit':
    case 'delete':
    case 'move':
      return 'file_edit';
    case 'execute':
      return VERIFYING.test(entry.text) ? 'verification' : 'command';
    case 'fetch':
      return 'fetch';
    default:
      return 'other';
  }
}

/** The file a call acted on, relative to the workspace when it can be. */
export function toolPath(entry: Entry): string | null {
  if (entry.diff) return entry.diff.path;
  const path = entry.data?.['path'];
  if (typeof path === 'string' && path) return path;
  const [, rest] = /^\S+\s+(.+)$/.exec(entry.title) ?? [];
  return rest ?? null;
}

/**
 * A phase's steps: consecutive actions fold into one group, and a retry or
 * a recovery between them stays inside it. Generation figures and the core's
 * own notes would repeat what the steps already say.
 */
export function groupSteps(entries: Entry[]): Step[] {
  const steps: Step[] = [];
  for (const entry of entries) {
    if (entry.kind === 'generation' || entry.kind === 'note') continue;
    // A streamed reply made only of whitespace is not a message.
    if (entry.kind === 'reply' && !entry.text.trim()) continue;
    const previous = steps[steps.length - 1];
    const joins = entry.kind === 'tool' || ((entry.kind === 'retry' || entry.kind === 'recovery') && previous?.type === 'actions');
    if (joins) {
      if (previous?.type === 'actions') previous.entries.push(entry);
      else steps.push({ type: 'actions', key: `group-${entry.key}`, entries: [entry], last: false });
    } else {
      steps.push({ type: 'entry', key: entry.key, entry });
    }
  }
  const lastGroup = [...steps].reverse().find((step) => step.type === 'actions');
  if (lastGroup && lastGroup.type === 'actions') lastGroup.last = true;
  return steps;
}

/**
 * Compact's view of a turn: its work as phases, and its answer.
 *
 * A phase is named by what the actions in it did -- reading before anything
 * changed is inspecting, an edit is implementing, a check is verifying, an
 * edit after a check is fixing -- and reasoning, retries and notes belong to
 * the phase they happened in. Text the model wrote before a retry was
 * discarded by the core, so it is never the result.
 */
export function compactTurn(entries: Entry[], live: boolean): CompactTurn {
  const visible = entries.filter((entry) => entry.kind !== 'generation' && !(entry.kind === 'reply' && !entry.text.trim()));
  let result: Entry | null = null;
  for (let index = visible.length - 1; index >= 0; index--) {
    const entry = visible[index];
    if (entry.kind === 'tool' || entry.kind === 'retry') break;
    if (entry.kind === 'reply') {
      result = entry;
      break;
    }
  }
  const phases: Phase[] = [];
  let edited = false;
  let checked = false;
  for (const entry of visible) {
    if (entry === result || entry.kind === 'stop') continue;
    const current = phases[phases.length - 1];
    let name: PhaseName = current?.name ?? 'Planning';
    if (entry.kind === 'tool') {
      switch (toolCategory(entry)) {
        case 'file_read':
        case 'search':
        case 'fetch':
          if (!current || current.name === 'Planning') name = 'Inspecting workspace';
          break;
        case 'file_edit':
          name = checked ? 'Fixing' : 'Implementing';
          edited = true;
          break;
        case 'verification':
          name = 'Verifying';
          checked = true;
          break;
        case 'command':
          // A command while inspecting (a listing, a version) is still inspecting.
          if (!current || current.name === 'Planning' || current.name === 'Verifying') name = checked ? 'Fixing' : 'Implementing';
          else if (current.name === 'Inspecting workspace' && edited) name = 'Implementing';
          break;
        default:
          break;
      }
    }
    if (current && current.name === name) {
      current.entries.push(entry);
      current.endedAt = Math.max(current.endedAt, entry.at);
    } else {
      phases.push({ key: `phase-${entry.key}`, name, entries: [entry], startedAt: entry.at, endedAt: entry.at });
    }
  }
  // Writing the answer after the work is its own phase while it happens.
  if (live && result && result.status === 'live' && phases.length) {
    phases.push({ key: `phase-final-${result.key}`, name: 'Finalizing', entries: [], startedAt: result.at, endedAt: result.at });
  }
  return { phases, result };
}

export interface PhaseSummary {
  read: number;
  edited: number;
  commands: number;
  checks: number;
  refused: number;
  retries: Entry[];
  recovered: number;
  /** Reasoning happened here; Compact says so, never what it was. */
  reasoned: boolean;
}

export function summarizePhase(phase: Phase): PhaseSummary {
  const summary: PhaseSummary = { read: 0, edited: 0, commands: 0, checks: 0, refused: 0, retries: [], recovered: 0, reasoned: false };
  for (const entry of phase.entries) {
    if (entry.kind === 'thought') summary.reasoned = true;
    if (entry.kind === 'retry') summary.retries.push(entry);
    if (entry.kind === 'recovery') summary.recovered += Number(entry.data?.['retries'] ?? 1);
    if (entry.kind !== 'tool') continue;
    switch (toolCategory(entry)) {
      case 'file_read':
      case 'search':
      case 'fetch':
        summary.read++;
        break;
      case 'file_edit':
        summary.edited++;
        break;
      case 'command':
        summary.commands++;
        break;
      case 'verification':
        summary.checks++;
        break;
      case 'refused':
        summary.refused++;
        break;
      default:
        break;
    }
  }
  return summary;
}

/** A retry in words a person reads while it happens, not the harness's own. */
export function retryLabel(cause: string): string {
  switch (cause) {
    case 'reply_fault':
    case 'malformed_call':
      return 'Tool call failed · retrying automatically';
    case 'backend_fault':
      return 'Engine error · retrying automatically';
    case 'silent':
      return 'Empty reply · asking again';
    case 'reasoning_unfinished':
      return 'Reasoning ran out of budget · asking for a direct answer';
    case 'context_limit':
      return 'Context full · reducing the window and retrying';
    default:
      return 'Retrying automatically';
  }
}

export function recoveredLabel(retries: number): string {
  return `Recovered after ${retries} ${retries === 1 ? 'retry' : 'retries'}`;
}

/** How a run ended, as the bottom of the conversation says it. */
export interface RunOutcome {
  /** PWR's terminal class, `null` for a turn that ended normally. */
  terminal: string | null;
  text: string;
  /** What the person can do about it: offered only once the core's own retries are spent. */
  action: 'retry' | 'continue' | null;
  /** The core's own words for why it stopped, shown in a phase's steps. */
  detail: string | null;
  tone: 'done' | 'paused' | 'stopped' | 'failed';
}

export function runOutcome(reply: any, cancelled: boolean): RunOutcome {
  const meta = reply?._meta?.pwr ?? {};
  const goal = meta.goal;
  const actions: number = meta.totalActions ?? meta.actions ?? 0;
  const detail: string | null = meta.stoppedBecause ?? null;
  const terminal: string | null = meta.terminal ?? null;
  const plural = (n: number) => `${n} action${n === 1 ? '' : 's'}`;
  const base = { terminal, detail };
  if (goal?.verified) return { ...base, text: `Goal verified by the declared acceptance checks after ${plural(actions)}.`, action: null, tone: 'done' };
  if (goal?.guardReached) return { ...base, text: `Goal mode paused after ${plural(actions)} without a verified completion.`, action: 'continue', tone: 'paused' };
  if (goal?.needsAcceptance)
    return { ...base, text: 'Technical checks passed; no acceptance contract was declared, so the goal is not verified.', action: null, tone: 'done' };
  if (cancelled || reply?.stopReason === 'cancelled' || terminal === 'interrupted') return { ...base, text: 'Stopped.', action: null, tone: 'stopped' };
  switch (terminal) {
    case 'budget':
      return { ...base, text: `Paused after ${plural(actions)} to check in. Nothing was discarded.`, action: 'continue', tone: 'paused' };
    case 'protocol':
      return { ...base, text: `Stopped after ${plural(actions)}: the model's replies stayed unusable after automatic retries.`, action: 'retry', tone: 'failed' };
    case 'provider':
      return { ...base, text: `Stopped after ${plural(actions)}: the engine kept failing after automatic retries.`, action: 'retry', tone: 'failed' };
    case 'recovery':
      return { ...base, text: `Stopped after ${plural(actions)}: the work was not moving. Say what to change.`, action: null, tone: 'failed' };
    case 'declined':
      return { ...base, text: 'The model declined the task.', action: null, tone: 'stopped' };
    default:
      return { ...base, text: `Finished after ${plural(actions)}.`, action: null, tone: 'done' };
  }
}

/** Runtime figures for the top bar; every one is `null` when nothing reported it. */
export interface RunMetrics {
  // The last generation.
  promptTokens: number | null;
  generatedTokens: number | null;
  reasoningTokens: number | null;
  promptEvalTps: number | null;
  generationTps: number | null;
  firstTokenMs: number | null;
  generationMs: number | null;
  tokenAccounting: string | null;
  // The run: everything since the person's last message.
  generations: number;
  runInputTokens: number | null;
  runOutputTokens: number | null;
  runMs: number | null;
  toolCalls: number;
  commands: number;
  filesRead: number;
  filesEdited: number;
  retries: number;
  recovered: number;
  verifications: number;
}

/** Figures for the run that `entries` end with, read from its events. */
export function runMetrics(entries: Entry[], now: number, live: boolean): RunMetrics {
  let start = 0;
  for (let index = entries.length - 1; index >= 0; index--) {
    if (entries[index].kind === 'user') {
      start = index;
      break;
    }
  }
  const run = entries.slice(start);
  const generations = run.filter((entry) => entry.kind === 'generation').map((entry) => entry.data ?? {});
  // The last generation of the conversation, so the figures stay after a new
  // message until its first reply arrives.
  const last = [...entries].reverse().find((entry) => entry.kind === 'generation')?.data ?? null;
  const num = (value: unknown): number | null => (typeof value === 'number' && isFinite(value) ? value : null);
  const rate = (tokens: unknown, ms: unknown): number | null => {
    const t = num(tokens);
    const m = num(ms);
    return t !== null && m !== null && m > 0 ? t / (m / 1000) : null;
  };
  const sum = (field: string): number | null => {
    const values = generations.map((g) => num(g[field])).filter((v): v is number => v !== null);
    return values.length ? values.reduce((a, b) => a + b, 0) : null;
  };
  const tools = run.filter((entry) => entry.kind === 'tool' && !isMalformed(entry));
  const read = new Set<string>();
  const edited = new Set<string>();
  let commands = 0;
  let verifications = 0;
  for (const entry of tools) {
    const category = toolCategory(entry);
    const path = toolPath(entry) ?? entry.key;
    if (category === 'file_read') read.add(path);
    if (category === 'file_edit' && entry.status === 'done') edited.add(path);
    if (category === 'command' || category === 'verification') commands++;
    if (category === 'verification') verifications++;
  }
  const first = run[0];
  const end = live ? now : (run[run.length - 1]?.at ?? null);
  return {
    promptTokens: num(last?.['promptTokens']),
    generatedTokens: num(last?.['generatedTokens']),
    reasoningTokens: num(last?.['reasoningTokens']),
    promptEvalTps: rate(last?.['promptTokens'], last?.['promptEvalMs']),
    generationTps: rate(last?.['generatedTokens'], last?.['generationMs']),
    firstTokenMs: num(last?.['firstChunkMs']),
    generationMs: num(last?.['generationMs']),
    tokenAccounting: last?.['tokenAccounting'] ?? null,
    generations: generations.length,
    runInputTokens: sum('promptTokens'),
    runOutputTokens: sum('generatedTokens'),
    runMs: first && end !== null && run.length > 1 ? Math.max(0, end - first.at) : null,
    toolCalls: tools.length,
    commands,
    filesRead: read.size,
    filesEdited: edited.size,
    retries: run.filter((entry) => entry.kind === 'retry').length,
    recovered: run.filter((entry) => entry.kind === 'recovery').reduce((n, entry) => n + Number(entry.data?.['retries'] ?? 1), 0),
    verifications,
  };
}

/** "1.2s", "340ms", "2m 05s". */
export function duration(ms: number | null): string {
  if (ms === null) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
  const whole = Math.round(seconds);
  return `${Math.floor(whole / 60)}m ${String(whole % 60).padStart(2, '0')}s`;
}

export function rate(value: number | null): string {
  if (value === null) return '—';
  return value >= 100 ? `${Math.round(value)}` : value.toFixed(1);
}
