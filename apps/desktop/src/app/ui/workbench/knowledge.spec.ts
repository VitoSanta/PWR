import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { AgentStore } from '../../core/agent.store';
import { PersonalStore } from '../../core/personal.store';
import { KnowledgeCard } from './knowledge';

const WIKI = { graph: { nodes: [], edges: [] }, modules: [], work: [], overview: '' };

describe('the Knowledge card', () => {
  let agent: AgentStore;
  let calls: { method: string; params: any }[];

  beforeEach(() => {
    TestBed.configureTestingModule({});
    agent = TestBed.inject(AgentStore);
    calls = [];
    vi.spyOn(agent, 'call').mockImplementation(async (method: string, params: any) => {
      calls.push({ method, params });
      if (method === '_pwr/projects') return { projects: TestBed.inject(PersonalStore).projects() };
      if (method === '_pwr/profile') return { profile: { memoryEnabled: true } };
      if (method === '_pwr/memory') return { global: [], workspace: [] };
      return WIKI;
    });
    const personal = TestBed.inject(PersonalStore);
    vi.spyOn(personal, 'load').mockResolvedValue();
    personal.projects.set([
      { name: 'ebooks', path: '/projects/ebooks', updatedAt: '2026-09-25' },
      { name: 'site', path: '/projects/site', updatedAt: '2026-09-20' },
    ]);
  });

  const wikiCalls = () => calls.filter((call) => call.method === '_pwr/wiki').map((call) => call.params);

  it('in chat mode, reads a project PWR knows, read only', async () => {
    agent.chatHome.set('/home/chat');
    agent.workspace.set('/home/chat');
    const fixture = TestBed.createComponent(KnowledgeCard);
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls().at(-1)).toMatchObject({ cwd: '/projects/ebooks', readOnly: true });
    // Another project, chosen.
    (fixture.componentInstance as any).choose('/projects/site');
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls().at(-1)).toMatchObject({ cwd: '/projects/site', readOnly: true });
    // Nothing that would write, or open a workspace card.
    expect(fixture.nativeElement.textContent).not.toContain('Write summaries');
  });

  it('in a workspace, reads that workspace', async () => {
    agent.workspace.set('/projects/app');
    const fixture = TestBed.createComponent(KnowledgeCard);
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls().at(-1)).toMatchObject({ cwd: '/projects/app', readOnly: false });
  });
});
