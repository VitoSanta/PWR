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
    agent.coreState.set('ready');
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

  it('waits for initialization and retries when the core becomes ready', async () => {
    agent.workspace.set('/projects/app');
    agent.coreState.set('starting');
    const fixture = TestBed.createComponent(KnowledgeCard);
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls()).toEqual([]);
    agent.coreState.set('ready');
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls()).toHaveLength(1);
  });

  it('keeps the latest request when symbol toggles finish out of order', async () => {
    agent.workspace.set('/projects/app');
    const fixture = TestBed.createComponent(KnowledgeCard);
    fixture.detectChanges();
    await fixture.whenStable();
    const pending: ((value: any) => void)[] = [];
    vi.mocked(agent.call).mockImplementation(() => new Promise(resolve => pending.push(resolve)));
    const card = fixture.componentInstance as any;
    const first = card.load();
    card.symbols.set(true);
    const latest = card.load();
    pending[1]({ ...WIKI, overview: 'Latest' });
    await latest;
    pending[0]({ ...WIKI, overview: 'Old' });
    await first;
    expect(card.wiki().overview).toBe('Latest');
    expect(card.loading()).toBe(false);
  });

  it('in a workspace, reads that workspace' , async () => {
    agent.workspace.set('/projects/app');
    const fixture = TestBed.createComponent(KnowledgeCard);
    fixture.detectChanges();
    await fixture.whenStable();
    expect(wikiCalls().at(-1)).toMatchObject({ cwd: '/projects/app', readOnly: false });
  });
});
