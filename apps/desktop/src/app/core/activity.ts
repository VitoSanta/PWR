import { Injectable, computed, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { ModelsStore, isTerminal } from './models.store';

/** Summaries being written for a workspace's wiki. */
export interface Summarising {
  cwd: string;
  state: 'started' | 'writing' | 'finished';
  module?: string;
  written: number;
}

/**
 * What runs in the background: wiki summaries, model downloads. Listened to
 * from the start, so the Activity card can show what began before it opened.
 */
@Injectable({ providedIn: 'root' })
export class ActivityStore {
  private readonly agent = inject(AgentStore);
  private readonly models = inject(ModelsStore);
  readonly summarising = signal<Summarising | null>(null);

  readonly downloads = computed(() =>
    Object.values(this.models.downloads()).filter((download) => !isTerminal(download)),
  );
  /** How many things are running now, for the card's badge. */
  readonly running = computed(
    () => (this.summarising() && this.summarising()!.state !== 'finished' ? 1 : 0) + this.downloads().length,
  );

  constructor() {
    this.agent.on('_pwr/wiki_summarising', (params) => {
      this.summarising.set({
        cwd: String(params.cwd ?? ''),
        state: params.state ?? 'started',
        module: params.module,
        written: Number(params.written ?? 0),
      });
    });
  }

  /** Writes the workspace's missing or stale summaries now. */
  summariseNow(): Promise<unknown> {
    return this.agent.call('_pwr/wiki_summarise', { cwd: this.agent.workspace() });
  }
}
