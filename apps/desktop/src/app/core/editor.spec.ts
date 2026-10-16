import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { AgentStore } from './agent.store';
import { EditorStore } from './editor';

describe('Editor buffers and checked saves', () => {
  const setup = () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/project');
    const call = vi.spyOn(agent,'call').mockImplementation(async method => method === '_pwr/files' ? {entries:[]} : {path:'a.ts',text:'one',hash:'v1'});
    const store = TestBed.inject(EditorStore); TestBed.tick();
    return {agent,store,call};
  };
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
});

describe('Editor folder operations', () => {
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
