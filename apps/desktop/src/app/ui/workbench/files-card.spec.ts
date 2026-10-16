import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { AgentStore } from '../../core/agent.store';
import { EditorStore } from '../../core/editor';

describe('Files request lifetime', () => {
  it('waits for the core and discards replies from a workspace since left', async () => {
    const agent = TestBed.inject(AgentStore);
    agent.workspace.set('/a');
    const call = vi.spyOn(agent, 'call').mockResolvedValue({ entries: [] });
    const card = TestBed.inject(EditorStore) as any;
    TestBed.tick();
    await Promise.resolve();
    expect(call).not.toHaveBeenCalled();
    agent.coreState.set('ready');
    TestBed.tick();
    await Promise.resolve();
    expect(call).toHaveBeenCalledWith('_pwr/files', { cwd: '/a', path: '' });
    let resolve!: (reply: unknown) => void;
    call.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const old = card.open('old.txt');
    agent.workspace.set('/b');
    TestBed.tick();
    await Promise.resolve();
    resolve({ path: 'old.txt', text: 'From A' });
    await old;
    expect(card.active()).toBeNull();
  });

  it('shows the last selected file when replies arrive in reverse order', async () => {
    const agent = TestBed.inject(AgentStore);
    agent.workspace.set('/a');
    const card = TestBed.inject(EditorStore) as any;
    TestBed.tick();
    const pending: ((reply: unknown) => void)[] = [];
    vi.spyOn(agent, 'call').mockImplementation(() => new Promise(done => pending.push(done)));
    const first = card.open('first.txt');
    const second = card.open('second.txt');
    pending[1]({ path: 'second.txt', text: 'Second' }); await second;
    pending[0]({ path: 'first.txt', text: 'First' }); await first;
    expect(card.active().path).toBe('second.txt');
  });
});
