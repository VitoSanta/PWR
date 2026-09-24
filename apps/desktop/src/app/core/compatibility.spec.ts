import {
  capabilityResult,
  offersFirstChoice,
  reasoningHelp,
  statusTitle,
  testedCapabilities,
} from './compatibility';
import { ModelCompatibility, ReasoningInfo } from './model';

const provisional: ModelCompatibility = {
  status: 'provisional',
  confidence: 'untested',
  reasons: [],
  capabilities: [
    { label: 'Tool calling', result: 'not_tested' },
    { label: 'Context', result: 'provisional' },
  ],
};

describe('compatibility', () => {
  it('offers the new-model choice once, for an untested model', () => {
    expect(statusTitle(provisional)).toBe('New model detected');
    expect(offersFirstChoice(provisional)).toBe(true);
    const acknowledged = { ...provisional, acknowledged: true };
    expect(offersFirstChoice(acknowledged)).toBe(false);
    expect(statusTitle(acknowledged)).toContain('conservative defaults');
    // Stale evidence says why rather than looking brand new.
    expect(offersFirstChoice({ ...provisional, reasons: ['quantization changed'] })).toBe(false);
    expect(offersFirstChoice({ ...provisional, status: 'limited' })).toBe(false);
  });

  it('never lists capabilities nobody tested', () => {
    expect(testedCapabilities(provisional)).toEqual([]);
    const calibrated: ModelCompatibility = {
      status: 'locally_calibrated',
      capabilities: [
        { label: 'Tool calling', result: 'supported' },
        { label: 'Context', result: 'provisional' },
      ],
    };
    expect(testedCapabilities(calibrated).length).toBe(2);
    expect(capabilityResult('not_reliable')).toBe('Not reliable');
  });

  it('describes the reasoning control without promising what it cannot do', () => {
    const budget: ReasoningInfo = {
      effort: 'medium',
      applies: true,
      control: 'budget',
      budgets: { low: 1024, medium: 4096, high: 8192 },
      budgetSource: 'conservative',
    };
    const help = reasoningHelp(budget);
    expect(help).toContain('Low 1k');
    expect(help).toContain('High 8k');
    expect(help).toContain('does not guarantee');
    expect(reasoningHelp({ ...budget, control: 'unknown', applies: false })).toContain(
      'does not expose a separately controllable reasoning phase',
    );
    expect(reasoningHelp({ ...budget, control: 'level' })).toContain('three native reasoning levels');
    expect(reasoningHelp(null)).toContain('compatible generation defaults');
  });
});
