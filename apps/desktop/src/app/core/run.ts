import { Injectable, computed, inject } from '@angular/core';
import { AgentStore } from './agent.store';
import { Entry } from './model';
import { Phase, PhaseName, PhaseSummary, compactTurn, summarizePhase } from './trace';

/** The agent's loop, in the order a run moves through it. */
export const PHASE_ORDER: PhaseName[] = [
  'Planning',
  'Inspecting workspace',
  'Implementing',
  'Verifying',
  'Fixing',
  'Finalizing',
];

/** Short names, for strips and board columns. */
export const PHASE_SHORT: Record<PhaseName, string> = {
  Planning: 'Plan',
  'Inspecting workspace': 'Inspect',
  Implementing: 'Implement',
  Verifying: 'Verify',
  Fixing: 'Fix',
  Finalizing: 'Answer',
};

export type RunState = 'idle' | 'working' | 'asking' | 'done' | 'paused' | 'stopped' | 'failed';

export const RUN_STATE_LABEL: Record<RunState, string> = {
  idle: 'Ready',
  working: 'Working',
  asking: 'Asking you',
  done: 'Done',
  paused: 'Paused',
  stopped: 'Stopped',
  failed: 'Failed',
};

/** One phase of the loop as the variants show it: reached or not, and what happened in it. */
export interface LoopStage {
  name: PhaseName;
  short: string;
  /** Every stretch of the run spent in this phase, in order. */
  phases: Phase[];
  entries: Entry[];
  summary: PhaseSummary | null;
  state: 'waiting' | 'active' | 'done';
}

/**
 * The latest run -- everything since the person's last message -- read the
 * way the conversation reads it (`compactTurn`), for the shells that show
 * the run beside the conversation rather than only inside it.
 */
@Injectable({ providedIn: 'root' })
export class RunStore {
  private readonly store = inject(AgentStore);

  /** The person's last message, and the entries of the turn that answers it. */
  private readonly turn = computed<{ prompt: Entry | null; entries: Entry[] }>(() => {
    const timeline = this.store.timeline();
    let start = 0;
    let prompt: Entry | null = null;
    for (let index = timeline.length - 1; index >= 0; index--) {
      if (timeline[index].kind === 'user') {
        prompt = timeline[index];
        start = index + 1;
        break;
      }
    }
    const entries = timeline
      .slice(start)
      .filter((entry) => !(entry.kind === 'notice' && entry.title !== 'Checkpoint'));
    return { prompt, entries };
  });

  readonly prompt = computed(() => this.turn().prompt);
  readonly entries = computed(() => this.turn().entries);
  readonly live = computed(() => this.store.turnActive());
  readonly compact = computed(() => compactTurn(this.entries(), this.live()));
  readonly phases = computed(() => this.compact().phases);
  readonly answer = computed(() => this.compact().result);

  /** The phase the run is in now, or ended in. */
  readonly current = computed<PhaseName | null>(() => {
    const phases = this.phases();
    return phases.length ? phases[phases.length - 1].name : null;
  });

  readonly state = computed<RunState>(() => {
    if (this.store.permission()) return 'asking';
    if (this.store.turnActive()) return 'working';
    const outcome = this.store.runOutcome();
    if (outcome) return outcome.tone;
    return this.entries().length ? 'done' : 'idle';
  });

  /**
   * The loop's stages with the run laid over them. Fixing appears only once
   * the run has had to fix something, and Answer only while it is writing
   * one: a clean run is plan, inspect, implement, verify.
   */
  readonly stages = computed<LoopStage[]>(() => {
    const phases = this.phases();
    const current = this.current();
    const live = this.live();
    const reached = new Set(phases.map((phase) => phase.name));
    return PHASE_ORDER.filter(
      (name) => (name !== 'Fixing' && name !== 'Finalizing') || reached.has(name),
    ).map((name) => {
      const own = phases.filter((phase) => phase.name === name);
      const entries = own.flatMap((phase) => phase.entries);
      const summary = own.length
        ? summarizePhase({ key: own[0].key, name, entries, startedAt: own[0].startedAt, endedAt: own[own.length - 1].endedAt })
        : null;
      const state: LoopStage['state'] =
        live && name === current ? 'active' : reached.has(name) ? 'done' : 'waiting';
      return { name, short: PHASE_SHORT[name], phases: own, entries, summary, state };
    });
  });

  /** The run's tool calls, in order. */
  readonly actions = computed(() => this.entries().filter((entry) => entry.kind === 'tool'));
}
