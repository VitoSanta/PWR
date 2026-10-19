import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import { UpdatesStore } from './updates';
import { AgentStore } from './agent.store';
import { EditorStore } from './editor';
import { TerminalService } from './terminal';
import { ConfirmService } from './ui';
import { bridge } from './bridge';
describe('signed updates', () => {
  let store: UpdatesStore, dirty: boolean;
  const active = signal(false), sessions = signal<unknown[]>([]);
  const confirm = { ask: vi.fn() };
  beforeEach(async () => {
    vi.restoreAllMocks();
    for (const name of ['updateStatus','onUpdateProgress','updateCheck','updateDownload','updateInstall','updateRestart','stop'] as const) vi.spyOn(bridge, name);
    localStorage.clear(); dirty = false; active.set(false); sessions.set([]);
    vi.mocked(bridge.updateStatus).mockResolvedValue({configured:true,currentVersion:'0.4.1-alpha',channel:'prerelease'});
    vi.mocked(bridge.onUpdateProgress).mockResolvedValue(() => {});
    vi.mocked(bridge.updateCheck).mockResolvedValue({version:'0.4.2-alpha',notes:'Fixes'});
    vi.mocked(bridge.updateDownload).mockResolvedValue(); vi.mocked(bridge.updateInstall).mockResolvedValue(); vi.mocked(bridge.updateRestart).mockResolvedValue(); vi.mocked(bridge.stop).mockResolvedValue();
    confirm.ask.mockReset().mockResolvedValue(true);
    TestBed.configureTestingModule({providers:[UpdatesStore,
      {provide:AgentStore,useValue:{turnActive:active,switchingWorkspace:signal(false),engineInstalling:signal(false)}},
      {provide:EditorStore,useValue:{hasUnsavedChanges:()=>dirty}}, {provide:TerminalService,useValue:{sessions}}, {provide:ConfirmService,useValue:confirm},
    ]});
    store=TestBed.inject(UpdatesStore); store.configured.set(true); await Promise.resolve();
  });
  afterEach(() => TestBed.resetTestingModule());
  async function ready() { await store.check(); await store.download(); }
  it('does not check automatically without opt-in', () => expect(bridge.updateCheck).not.toHaveBeenCalled());
  it('never installs after failed signature/download verification', async () => {
    await store.check(); vi.mocked(bridge.updateDownload).mockRejectedValue(new Error('invalid signature'));
    await store.download(); await store.install(); expect(store.phase()).toBe('available'); expect(bridge.updateInstall).not.toHaveBeenCalled();
  });
  it('rechecks unsaved buffers after confirmation', async () => {
    await ready(); confirm.ask.mockImplementation(async()=>{dirty=true;return true;});
    await store.install(); expect(bridge.stop).not.toHaveBeenCalled(); expect(bridge.updateInstall).not.toHaveBeenCalled();
  });
  it('blocks active turns and terminal sessions', async () => {
    await ready(); active.set(true); await store.install(); active.set(false); sessions.set([{}]); await store.install();
    expect(confirm.ask).not.toHaveBeenCalled(); expect(bridge.updateInstall).not.toHaveBeenCalled();
  });
  it('stops the core before install and restarts only after success', async () => {
    await ready(); await store.install(); expect(bridge.stop).toHaveBeenCalledOnce(); expect(bridge.updateInstall).toHaveBeenCalledOnce(); expect(bridge.updateRestart).toHaveBeenCalledOnce(); expect(store.phase()).toBe('installed');
  });
  it('serializes two confirmations resolved together', async () => {
    await ready(); let resolve!: (value:boolean)=>void;
    const answer = new Promise<boolean>(done=>resolve=done); confirm.ask.mockReturnValue(answer);
    const first=store.install(), second=store.install(); resolve(true); await Promise.all([first,second]);
    expect(bridge.updateInstall).toHaveBeenCalledOnce(); expect(bridge.updateRestart).toHaveBeenCalledOnce();
  });
  it('does not restart after an installation failure' , async () => {
    await ready(); vi.mocked(bridge.updateInstall).mockRejectedValue(new Error('disk full')); await store.install(); expect(bridge.updateRestart).not.toHaveBeenCalled(); expect(store.phase()).toBe('ready');
  });
  it('does not stop the workspace when confirmation is cancelled', async () => {
    await ready(); confirm.ask.mockResolvedValue(false); await store.install(); expect(bridge.stop).not.toHaveBeenCalled();
  });
});
