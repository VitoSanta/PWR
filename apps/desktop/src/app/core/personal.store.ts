import { Injectable, inject, signal } from '@angular/core';
import { AgentStore } from './agent.store';
import { Memory, MemoryList, MemoryProposal, MemoryScope, Profile } from './model';

/**
 * The person's profile and what PWR remembers, kept by the core in
 * `~/.pwr/profile.json`, `~/.pwr/memory.json` and the workspace's
 * `.pwr/memory.json`. A model only proposes a memory; it is saved here, when
 * the person confirms it.
 */
@Injectable({ providedIn: 'root' })
export class PersonalStore {
  private readonly agent = inject(AgentStore);

  readonly profile = signal<Profile>({ memoryEnabled: true });
  readonly memories = signal<MemoryList>({ global: [], workspace: [], instructions: null });
  readonly proposals = signal<MemoryProposal[]>([]);
  readonly loading = signal(false);
  readonly saving = signal(false);
  readonly error = signal('');

  constructor() {
    this.agent.on('_pwr/memory_proposed', (params) => {
      const text = String(params.text ?? '').trim();
      if (!text) return;
      const scope: MemoryScope = params.scope === 'global' ? 'global' : 'workspace';
      this.proposals.update((all) =>
        all.some((proposal) => proposal.text === text)
          ? all
          : [...all, { key: `${Date.now()}-${all.length}`, sessionId: params.sessionId ?? null, text, scope }],
      );
    });
  }

  /** Reads the profile and both memory lists. */
  async load(): Promise<void> {
    this.loading.set(true);
    this.error.set('');
    try {
      const [profile, memories] = await Promise.all([
        this.agent.call('_pwr/profile', {}),
        this.agent.call('_pwr/memory', { cwd: this.agent.workspace() }),
      ]);
      this.profile.set({ memoryEnabled: true, ...(profile.profile ?? {}) });
      this.memories.set(memories as MemoryList);
    } catch (error) {
      this.error.set(message(error));
    } finally {
      this.loading.set(false);
    }
  }

  async saveProfile(profile: Profile): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    try {
      const reply = await this.agent.call('_pwr/profile', { profile });
      this.profile.set({ memoryEnabled: true, ...(reply.profile ?? {}) });
    } catch (error) {
      this.error.set(message(error));
    } finally {
      this.saving.set(false);
    }
  }

  add(scope: MemoryScope, text: string, source?: string): Promise<void> {
    return this.change({ action: 'add', scope, text, source });
  }

  update(scope: MemoryScope, memory: Memory, text: string): Promise<void> {
    return this.change({ action: 'update', scope, id: memory.id, text });
  }

  remove(scope: MemoryScope, memory: Memory): Promise<void> {
    return this.change({ action: 'delete', scope, id: memory.id });
  }

  /** Saves a proposal the person confirmed, in the scope they chose. */
  async accept(proposal: MemoryProposal, scope: MemoryScope = proposal.scope): Promise<void> {
    await this.add(scope, proposal.text, proposal.sessionId ?? undefined);
    if (!this.error()) this.dismiss(proposal);
  }

  dismiss(proposal: MemoryProposal): void {
    this.proposals.update((all) => all.filter((item) => item.key !== proposal.key));
  }

  private async change(params: Record<string, unknown>): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    try {
      const reply = await this.agent.call('_pwr/memory', { cwd: this.agent.workspace(), ...params });
      this.memories.set(reply as MemoryList);
    } catch (error) {
      this.error.set(message(error));
    } finally {
      this.saving.set(false);
    }
  }
}

function message(error: unknown): string {
  return String(error).replace(/^Error: /, '');
}
