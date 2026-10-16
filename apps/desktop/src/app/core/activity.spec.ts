import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { ActivityStore } from './activity';
import { AgentStore } from './agent.store';

describe('Summary activity failures', () => {
  it('shows a failed job as stopped and preserves the reason', () => {
    const agent = TestBed.inject(AgentStore);
    const activity = TestBed.inject(ActivityStore);
    agent.receive({ method: '_pwr/wiki_summarising', params: { cwd: '/app', state: 'writing', written: 0 } });
    expect(activity.running()).toBe(1);
    agent.receive({ method: '_pwr/wiki_summarising', params: { cwd: '/app', state: 'failed', written: 0, error: 'Engine unavailable' } });
    expect(activity.running()).toBe(0);
    expect(activity.summarising()?.error).toBe('Engine unavailable');
  });
  it('does not silently accept a summary request that did not start', async () => {
    const agent = TestBed.inject(AgentStore);
    vi.spyOn(agent, 'call').mockResolvedValue({ started: false });
    await expect(TestBed.inject(ActivityStore).summariseNow()).rejects.toThrow('core is busy');
  });
});
