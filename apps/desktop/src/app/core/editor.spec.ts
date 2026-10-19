import { signal } from '@angular/core';
import { DraftRecovery } from './draft-recovery';
import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { EditorStore } from './editor';
import { WorkbenchStore } from './workbench';
import { ConfirmService } from './ui';

describe('Editor buffers and checked saves', () => {
  const setup = () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    const call = vi.spyOn(agent,'call').mockImplementation(async method => method === '_pwr/files' ? {entries:[]} : {path:'a.ts',text:'one',hash:'v1'});
    const store = TestBed.inject(EditorStore); TestBed.tick();
    return {agent,store,call};
  };
  it('refreshes the visible explorer even when no file is open', async () => {
    vi.useFakeTimers();
    try {
      TestBed.configureTestingModule({providers:[{provide:WorkbenchStore,useValue:{fileRequest:signal(null),panelVisible:()=>true,visible:()=>[{id:'files'}]}}]});
      const {agent,store,call}=setup();
      agent.coreState.set('ready'); TestBed.tick(); await Promise.resolve();
      call.mockImplementation(async method => method === '_pwr/files' ? {entries:[{path:'new.ts',name:'new.ts',dir:false,bytes:3}]} : {});
      await vi.advanceTimersByTimeAsync(2501);
      expect(store.documents()).toEqual([]);
      expect(store.entries().map(entry=>entry.path)).toContain('new.ts');
    } finally {vi.useRealTimers();}
  });
  it('refreshes later buffers when an earlier file was deleted', async () => {
    const {store,call}=setup();await store.open('a.ts');await store.open('b.ts');
    call.mockImplementation(async (_method,args:any)=>{if(args.path==='a.ts')throw new Error('deleted');return {text:'external',hash:'v2'};});
    await (store as any).refresh();
    expect(store.documents().find(doc=>doc.path==='b.ts')?.text).toBe('external');
  });
  it('saves the version read and preserves edits made during the save', async () => {
    const {store,call} = setup(); await store.open('a.ts'); store.edit('a.ts','two');
    let complete!: (value:unknown)=>void;
    call.mockImplementationOnce(() => new Promise(resolve=>{complete=resolve;}));
    const save=store.save();store.edit('a.ts','three');complete({hash:'v2'});await save;
    expect(call).toHaveBeenLastCalledWith('_pwr/editor',expect.objectContaining({hash:'v1',text:'two'}));
    expect(store.active()?.text).toBe('three');expect(store.active()?.saved).toBe('two');expect(store.dirty(store.active()!)).toBe(true);
  });
  it('blocks model writes that overlap unsaved text', async () => {
    const {agent,store}=setup();await store.open('a.ts');store.edit('a.ts','unsaved');
    const guard=agent.workspaceHandlers.get('editor')!;
    await expect(guard('guard',{path:'a.ts'})).rejects.toThrow('Unsaved');
    await expect(guard('guard',{})).rejects.toThrow('Unsaved');
    await expect(guard('guard',{path:'b.ts'})).resolves.toEqual({allowed:true});
  });
  it('marks external modifications as conflicts while retaining dirty buffers', async () => {
    const {store,call}=setup();await store.open('a.ts');store.edit('a.ts','unsaved');
    call.mockResolvedValue({path:'a.ts',text:'external',hash:'v2'});
    await (store as any).refresh();
    expect(store.active()).toMatchObject({text:'unsaved',saved:'one',hash:'v1',conflict:true});
  });
  it('retains drafts for the original workspace without exposing them to the next', async () => {
    const {agent,store}=setup();await store.open('a.ts');store.edit('a.ts','draft');
    agent.workspace.set('/other');TestBed.tick();expect(store.documents()).toEqual([]);
    agent.workspace.set('/project');TestBed.tick();expect(store.active()?.text).toBe('draft');
  });
  it('checks hidden workspace drafts without counting a stale active-workspace snapshot', async () => {
    const {agent,store}=setup(); await store.open('a.ts'); store.edit('a.ts','draft');
    agent.workspace.set('/other'); TestBed.tick(); expect(store.hasUnsavedChanges()).toBe(true);
    agent.workspace.set('/project'); TestBed.tick(); store.edit('a.ts','one');
    expect(store.hasUnsavedChanges()).toBe(false);
  });
  for (const operation of ['close', 'reload'] as const) {
    it(`ignores a ${operation} confirmation after switching workspace`, async () => {
      const {agent,store,call}=setup();await store.open('a.ts');store.edit('a.ts','draft A');
      let answer!: (value:boolean)=>void;
      vi.spyOn(TestBed.inject(ConfirmService),'ask').mockImplementationOnce(() => new Promise(resolve => {answer=resolve;}));
      const pending=operation==='close' ? store.close(store.active()!) : store.reload();
      agent.workspace.set('/other');TestBed.tick();await store.open('a.ts');store.edit('a.ts','draft B');
      const calls=call.mock.calls.length;answer(true);await pending;
      expect(store.active()?.text).toBe('draft B');expect(call.mock.calls.length).toBe(calls);
      agent.workspace.set('/project');TestBed.tick();expect(store.active()?.text).toBe('draft A');
    });
    it(`preserves edits made while a ${operation} confirmation is pending`, async () => {
      const {store}=setup();await store.open('a.ts');store.edit('a.ts','first');
      let answer!: (value:boolean)=>void;
      vi.spyOn(TestBed.inject(ConfirmService),'ask').mockImplementationOnce(() => new Promise(resolve => {answer=resolve;}));
      const pending=operation==='close' ? store.close(store.active()!) : store.reload();
      store.edit('a.ts','newer');answer(true);await pending;
      expect(store.active()?.text).toBe('newer');
    });
  }
  it('requires explicit discard before quitting with a draft in a previous workspace', async () => {
    const {agent,store}=setup();
    const ask=vi.spyOn(TestBed.inject(ConfirmService),'ask').mockResolvedValue(false);
    expect(await store.allowAppClose()).toBe(true);
    expect(ask).not.toHaveBeenCalled();
    await store.open('a.ts');store.edit('a.ts','draft');
    agent.workspace.set('/other');TestBed.tick();
    expect(await store.allowAppClose()).toBe(false);
    expect(ask).toHaveBeenCalledWith(expect.objectContaining({confirmLabel:'Discard and quit'}));
    ask.mockResolvedValue(true);
    expect(await store.allowAppClose()).toBe(true);
  });
});

describe('Editor folder operations', () => {
  it('obtains a binary file version without opening a text buffer before rename', async () => {
    const agent=TestBed.inject(AgentStore);agent.workspace.set('/project');
    const call=vi.spyOn(agent,'call').mockImplementation(async(method,params:any)=>{
      if(method==='_pwr/files')return {entries:[]};
      if(params.operation==='stat')return {hash:'binary-version',dir:false};
      return {path:'new.bin'};
    });
    const store=TestBed.inject(EditorStore);TestBed.tick();
    await store.mutate('rename','old.bin','new.bin');
    expect(call).toHaveBeenCalledWith('_pwr/editor',expect.objectContaining({operation:'rename',hash:'binary-version'}));
    expect(call.mock.calls.some(([method])=>method==='_pwr/file')).toBe(false);
    expect(store.documents()).toEqual([]);
  });
  it('removes expanded folders from navigation before reloading their parent', async () => {
    const agent=TestBed.inject(AgentStore);agent.workspace.set('/project');
    const calls: any[]=[];
    vi.spyOn(agent,'call').mockImplementation(async(method,params:any)=>{
      calls.push({method,params});
      if(method==='_pwr/file')return {path:params.path,text:'saved',hash:'v1'};
      if(method==='_pwr/editor')return {restoreToken:'token'};
      if(params.path==='folder')throw new Error('Folder was removed');
      return {entries:[]};
    });
    const store=TestBed.inject(EditorStore);TestBed.tick();
    await store.open('keep.txt');await store.open('folder/a.txt');store.expanded.set(['folder']);
    await store.mutate('trash','folder');
    expect(store.expanded()).toEqual([]);expect(store.activePath()).toBe('keep.txt');expect(store.error()).toBe('');
    expect(calls.filter(call=>call.method==='_pwr/files').at(-1)?.params.path).toBe('');
  });
});


describe('Durable editor recovery', () => {
  const setup = (read = vi.fn().mockResolvedValue(null)) => {
    const recovery = {enabled: true, error: signal(''), read, write: vi.fn().mockResolvedValue(undefined), flush: vi.fn().mockResolvedValue(undefined)};
    TestBed.configureTestingModule({providers: [{provide: DraftRecovery, useValue: recovery}]});
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    const call = vi.spyOn(agent, 'call').mockImplementation(async method => method === '_pwr/files' ? {entries: []} : {text: 'disk', hash: 'v1'});
    const store = TestBed.inject(EditorStore); TestBed.tick();
    return {store, agent, recovery, call};
  };
  const saved = () => ({version: 1, workspace: '/project', active: 'a.ts', documents: [{path: 'a.ts', text: 'recovered draft', saved: 'base', hash: 'old', conflict: false}]});
  it('restores the original save hash and never writes the workspace file', async () => {
    const {store, call} = setup(vi.fn().mockResolvedValue(saved()));
    await store.open('a.ts');
    expect(store.active()).toMatchObject({text: 'recovered draft', hash: 'old', saved: 'base'});
    expect(store.recoveryMessage()).toContain('Recovered');
    expect(call.mock.calls.some(([method]) => method === '_pwr/editor')).toBe(false);
  });
  it('keeps a delayed recovery in its own workspace', async () => {
    let resolve!: (value: unknown) => void;
    const read = vi.fn().mockImplementation((cwd: string) => cwd === '/project' ? new Promise(done => { resolve = done; }) : Promise.resolve(null));
    const {store, agent} = setup(read);
    agent.workspace.set('/other'); TestBed.tick(); await store.open('a.ts'); store.edit('a.ts', 'other draft');
    resolve(saved()); await Promise.resolve(); TestBed.tick();
    expect(store.active()?.text).toBe('other draft');
    agent.workspace.set('/project'); TestBed.tick(); await store.open('a.ts');
    expect(store.active()?.text).toBe('recovered draft');
  });
  it('persists a draft immediately when switching before the debounce', async () => {
    const {store, agent, recovery} = setup(); await store.open('a.ts'); store.edit('a.ts', 'new draft');
    agent.workspace.set('/other'); TestBed.tick(); await Promise.resolve();
    expect(recovery.write).toHaveBeenCalledWith('/project', expect.objectContaining({documents: [expect.objectContaining({text: 'new draft'})]}));
  });
  it('retains newer edits when a quit confirmation resolves', async () => {
    const {store, recovery} = setup(); await store.open('a.ts'); store.edit('a.ts', 'first');
    let answer!: (value: boolean) => void;
    vi.spyOn(TestBed.inject(ConfirmService), 'ask').mockImplementation(() => new Promise(done => {answer = done;}));
    const pending = store.allowAppClose(); await Promise.resolve(); await Promise.resolve();
    store.edit('a.ts', 'newer'); answer(true);
    expect(await pending).toBe(false); expect(store.active()?.text).toBe('newer');
    expect(recovery.write.mock.calls.some(([, snapshot]) => snapshot === null)).toBe(false);
  });
  it('explicit discard waits for queued copies and clears their records', async () => {
    const {store, recovery} = setup(); await store.open('a.ts'); store.edit('a.ts', 'draft');
    vi.spyOn(TestBed.inject(ConfirmService), 'ask').mockResolvedValue(true);
    expect(await store.allowAppClose()).toBe(true);
    expect(recovery.flush).toHaveBeenCalled(); expect(recovery.write).toHaveBeenLastCalledWith('/project', null);
  });
  it('refuses to quit if clearing the recovery copy fails', async () => {
    const {store, recovery} = setup(); await store.open('a.ts'); store.edit('a.ts', 'draft');
    vi.spyOn(TestBed.inject(ConfirmService), 'ask').mockResolvedValue(true);
    recovery.write.mockRejectedValue(new Error('disk full'));
    expect(await store.allowAppClose()).toBe(false); expect(store.error()).toContain('disk full');
  });
});


describe('Editor quit recovery races', () => {
  it('rewrites hidden drafts if a new edit arrives while discard copies are being cleared', async () => {
    let release!: () => void;
    const recovery = {enabled: true, error: signal(''), read: vi.fn().mockResolvedValue(null), write: vi.fn().mockResolvedValue(undefined), flush: vi.fn().mockResolvedValue(undefined)};
    TestBed.configureTestingModule({providers: [{provide: DraftRecovery, useValue: recovery}]});
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    vi.spyOn(agent, 'call').mockResolvedValue({text: 'base', hash: 'v1'});
    const store = TestBed.inject(EditorStore); TestBed.tick(); await store.open('a.ts'); store.edit('a.ts', 'hidden draft');
    agent.workspace.set('/other'); TestBed.tick(); await store.open('b.ts'); store.edit('b.ts', 'first');
    vi.spyOn(TestBed.inject(ConfirmService), 'ask').mockResolvedValue(true);
    recovery.write.mockImplementation((_: string, value: unknown) => value === null ? new Promise<void>(resolve => { release = resolve; }) : Promise.resolve());
    const pending = store.allowAppClose();
    for (let i = 0; i < 8 && !release; i++) await Promise.resolve();
    store.edit('b.ts', 'newer'); release();
    // There are two records to clear; release the second one too.
    const firstRelease = release;
    for (let i = 0; i < 8 && release === firstRelease; i++) await Promise.resolve();
    release();
    expect(await pending).toBe(false);
    expect(recovery.write).toHaveBeenCalledWith('/project', expect.objectContaining({documents: [expect.objectContaining({text: 'hidden draft'})]}));
    expect(recovery.write).toHaveBeenCalledWith('/other', expect.objectContaining({documents: [expect.objectContaining({text: 'newer'})]}));
  });
});


describe('Editor quit approvals', () => {
  it('does not invalidate approved text merely because conflict polling refreshes metadata', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    vi.spyOn(agent, 'call').mockResolvedValue({text: 'base', hash: 'v1'});
    const store = TestBed.inject(EditorStore); TestBed.tick(); await store.open('a.ts'); store.edit('a.ts', 'draft');
    let answer!: (value: boolean) => void;
    vi.spyOn(TestBed.inject(ConfirmService), 'ask').mockImplementation(() => new Promise(done => {answer = done;}));
    const pending = store.allowAppClose(); await Promise.resolve(); await Promise.resolve();
    store.documents.update(docs => docs.map(doc => ({...doc, conflict: true})));
    answer(true); expect(await pending).toBe(true);
  });
});


describe('Pending draft recovery boundaries', () => {
  it('blocks updater and model writes until a delayed dirty recovery is known', async () => {
    let resolve!: (value: unknown) => void;
    const recovery = {enabled: true, error: signal(''), read: vi.fn().mockImplementation(() => new Promise(done => {resolve = done;})), write: vi.fn().mockResolvedValue(undefined), flush: vi.fn().mockResolvedValue(undefined)};
    TestBed.configureTestingModule({providers: [{provide: DraftRecovery, useValue: recovery}]});
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    const store = TestBed.inject(EditorStore); TestBed.tick();
    expect(store.hasUnsavedChanges()).toBe(true);
    let settled = false;
    const guard = agent.workspaceHandlers.get('editor')!('guard', {path: 'a.ts'});
    const outcome = guard.then(() => {settled = true; return '';}, error => {settled = true; return String(error);});
    await Promise.resolve(); expect(settled).toBe(false);
    resolve({version: 1, workspace: '/project', active: 'a.ts', documents: [{path: 'a.ts', text: 'draft', saved: 'base', hash: 'v1', conflict: false}]});
    expect(await outcome).toContain('Unsaved');
  });
});
