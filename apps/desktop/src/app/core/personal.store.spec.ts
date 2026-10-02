import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { PersonalStore } from './personal.store';

describe('Memory proposal origin', () => {
  it('saves to the originating workspace after the visible workspace changes', async () => {
    let cwd = '/project-a';
    const handlers = new Map<string, (params: any) => void>();
    const agent = { workspace: () => cwd, on: (name: string, handler: (params: any) => void) => handlers.set(name, handler),
      call: vi.fn().mockResolvedValue({ global: [], workspace: [], instructions: null }) };
    TestBed.configureTestingModule({ providers: [{ provide: AgentStore, useValue: agent }] });
    const store = TestBed.inject(PersonalStore);
    handlers.get('_pwr/memory_proposed')!({ text: 'Uses pnpm', scope: 'workspace', cwd, sessionId: 'a' });
    cwd = '/project-b';
    await store.accept(store.proposals()[0]);
    expect(agent.call).toHaveBeenCalledWith('_pwr/memory', {
      cwd: '/project-a', action: 'add', scope: 'workspace', text: 'Uses pnpm', source: 'a',
    });
    expect(store.proposals()).toEqual([]);
  });

  it('keeps identical facts from distinct workspaces and refuses unknown origins', () => {
    const handlers = new Map<string, (params: any) => void>();
    const agent = { workspace: () => '/project-b', on: (name: string, handler: (params: any) => void) => handlers.set(name, handler) };
    TestBed.configureTestingModule({ providers: [{ provide: AgentStore, useValue: agent }] });
    const store = TestBed.inject(PersonalStore);
    const receive = handlers.get('_pwr/memory_proposed')!;
    receive({ text: 'Uses pnpm', scope: 'workspace', cwd: '/project-a' });
    receive({ text: 'Uses pnpm', scope: 'workspace', cwd: '/project-b' });
    receive({ text: 'Unknown origin', scope: 'workspace' });
    expect(store.proposals().map(p => p.cwd)).toEqual(['/project-a', '/project-b']);
  });
});
