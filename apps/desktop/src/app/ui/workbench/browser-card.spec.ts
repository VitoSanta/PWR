import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { BrowserStore, browserAddress } from '../../core/browser';
import { AgentStore } from '../../core/agent.store';
import { bridge } from '../../core/bridge';
import { BrowserCard } from './browser';

describe('Shared browser tabs', () => {
  beforeEach(() => {
    TestBed.overrideComponent(BrowserCard, {set:{template:'',imports:[]}});
    vi.spyOn(bridge, 'browserOpen').mockResolvedValue();
    vi.spyOn(bridge, 'browserClose').mockResolvedValue();
  });
  it('shows explicit addresses without requiring a search-engine choice', async () => {
    const agent=TestBed.inject(AgentStore);agent.workspace.set('/a');
    const store=TestBed.inject(BrowserStore);TestBed.tick();store.choosing.set(true);
    await store.open('http://localhost:8765');expect(store.choosing()).toBe(false);
    store.choosing.set(true);await store.navigate('http://localhost:8765/next');
    expect(store.choosing()).toBe(false);expect(store.engine()).toBeNull();
  });
  it('closes a navigation that finishes after switching workspace', async () => {
    const agent=TestBed.inject(AgentStore);agent.workspace.set('/a');
    const store=TestBed.inject(BrowserStore);TestBed.tick();const tab=await store.open('http://localhost:8765');
    let complete!: ()=>void;
    vi.spyOn(bridge,'browserOpen').mockImplementationOnce(()=>new Promise<void>(resolve=>{complete=resolve;}));
    const navigation=store.navigate('http://localhost:8765/late');
    agent.workspace.set('/b');TestBed.tick();complete();
    await expect(navigation).rejects.toThrow('workspace changed');
    expect(store.tabs()).toEqual([]);expect(bridge.browserClose).toHaveBeenCalledWith(tab.id);
  });
  it('keeps the same native tab when a card is destroyed and recreated', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a');
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('localhost:5199/tickets');
    let fixture = TestBed.createComponent(BrowserCard); fixture.detectChanges(); fixture.destroy();
    fixture = TestBed.createComponent(BrowserCard); fixture.detectChanges();
    expect(store.active()?.id).toBe(tab.id);
    expect(store.active()?.url).toBe('http://localhost:5199/tickets');
    fixture.destroy();
  });
  it('closes tabs when the workspace changes', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a');
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('https://example.com');
    agent.workspace.set('/b'); TestBed.tick();
    expect(store.tabs()).toEqual([]);
    expect(bridge.browserClose).toHaveBeenCalledWith(tab.id);
  });
  it('does not dispatch model input after the engineer takes control', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a'); agent.sessionId.set('s'); agent.turnActive.set(true);
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('https://example.com','model'); store.control('user');
    const action = vi.spyOn(bridge,'browserAction').mockResolvedValue({});
    await expect(agent.workspaceHandlers.get('browser')!('click',{tabId:tab.id,ref:'x'})).rejects.toThrow('controlled by the engineer');
    expect(action).not.toHaveBeenCalled();
  });
  it('serves the conversation that is asking when control was given before it had an id', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a'); agent.turnActive.set(true);
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('https://example.com');
    store.control('model');
    expect(store.active()?.conversationId).toBeNull();
    agent.sessionId.set('first');
    const action = vi.spyOn(bridge,'browserAction').mockResolvedValue({ text: 'page' });
    await expect(agent.workspaceHandlers.get('browser')!('snapshot',{tabId:tab.id})).resolves.toEqual({ text: 'page' });
    expect(store.active()?.conversationId).toBe('first');
    expect(action).toHaveBeenCalledOnce();
    // Another conversation is still turned away.
    agent.sessionId.set('second');
    await expect(agent.workspaceHandlers.get('browser')!('snapshot',{tabId:tab.id})).rejects.toThrow('controlled by the engineer');
  });
  it('asks again while the page view is still appearing', async () => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a'); agent.sessionId.set('s'); agent.turnActive.set(true);
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('https://example.com','model');
    const action = vi.spyOn(bridge,'browserAction');
    action.mockClear();
    action.mockRejectedValueOnce('This browser tab must be visible before interacting with it.').mockResolvedValue({ ok: true });
    await expect(agent.workspaceHandlers.get('browser')!('snapshot',{tabId:tab.id})).resolves.toEqual({ ok: true });
    expect(action).toHaveBeenCalledTimes(2);
    expect(store.error()).toBe('');
  });
  it.each(['tab', 'control', 'cancel', 'session', 'workspace'])('does not retry after %s changes', async (change) => {
    const agent = TestBed.inject(AgentStore); agent.workspace.set('/a'); agent.sessionId.set('s'); agent.turnActive.set(true);
    const store = TestBed.inject(BrowserStore); TestBed.tick();
    const tab = await store.open('https://example.com', 'model');
    const user = await store.open('https://example.org', 'user');
    const action = vi.spyOn(bridge, 'browserAction').mockClear().mockImplementationOnce(async () => {
      if (change === 'tab') store.select(user.id);
      if (change === 'control') store.control('user');
      if (change === 'cancel') agent.cancel();
      if (change === 'session') agent.sessionId.set('other');
      if (change === 'workspace') agent.workspace.set('/b');
      throw new Error('This browser tab must be visible before interacting with it.');
    }).mockResolvedValue({ acted: true });
    vi.spyOn(bridge, 'send').mockResolvedValue();
    await expect(agent.workspaceHandlers.get('browser')!('reload', { tabId: tab.id })).rejects.toThrow();
    expect(action).toHaveBeenCalledTimes(1);
    expect(action.mock.calls[0][0]).toBe(tab.id);
  });
  it('rejects host protocols, embedded credentials and PWR origins', () => {
    for (const address of ['file:///etc/passwd','javascript://alert(1)','https://u:p@example.com','http://ipc.localhost']) expect(() => browserAddress(address)).toThrow();
    expect(browserAddress('example.com')).toBe('https://example.com/');
  });
});
