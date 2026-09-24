import { tokens } from './format';
import { CapabilityLine, ModelCompatibility, ReasoningEffort, ReasoningInfo } from './model';

/** The words a capability result is shown with. */
export function capabilityResult(result: CapabilityLine['result']): string {
  switch (result) {
    case 'supported':
      return 'Supported';
    case 'not_reliable':
      return 'Not reliable';
    case 'detected':
      return 'Detected';
    case 'not_detected':
      return 'Not detected';
    case 'provisional':
      return 'Provisional';
    default:
      return 'Not tested';
  }
}

export function statusTitle(compatibility: ModelCompatibility): string {
  switch (compatibility.status) {
    case 'verified':
      return 'Verified';
    case 'locally_calibrated':
      return 'Locally calibrated';
    case 'limited':
      return 'Limited compatibility';
    case 'incompatible':
      return 'Incompatible';
    default:
      return compatibility.acknowledged ? 'Untested — conservative defaults' : 'New model detected';
  }
}

export function confidenceLabel(compatibility: ModelCompatibility): string {
  switch (compatibility.confidence) {
    case 'established':
      return 'Established';
    case 'preliminary':
      return 'Preliminary';
    case 'reduced':
      return 'Reduced — evidence from a different setup';
    default:
      return 'Untested';
  }
}

/** Offer the new-model choice only once, and only for an untested model. */
export function offersFirstChoice(compatibility: ModelCompatibility | null): boolean {
  return (
    !!compatibility &&
    compatibility.status === 'provisional' &&
    !compatibility.acknowledged &&
    !(compatibility.reasons ?? []).length
  );
}

/** Whether any capability was actually tested: untested lines are not listed. */
export function testedCapabilities(compatibility: ModelCompatibility | null): CapabilityLine[] {
  const lines = compatibility?.capabilities ?? [];
  return lines.some((line) => line.result !== 'not_tested' && line.result !== 'provisional')
    ? lines
    : [];
}

export const EFFORT_HELP: Record<ReasoningEffort, string> = {
  low: 'Faster responses with a smaller reasoning budget.',
  medium: 'Balanced default.',
  high: 'Allows a larger reasoning budget for difficult tasks.',
};

/** What the Reasoning Effort control does for this model, honestly. */
export function reasoningHelp(info: ReasoningInfo | null): string {
  switch (info?.control) {
    case 'budget': {
      const budgets = info.budgets;
      const levels = budgets
        ? `Thinking budget: Low ${tokens(budgets.low)} · Medium ${tokens(budgets.medium)} · High ${tokens(budgets.high)} tokens`
        : 'Sets the thinking budget';
      const source =
        info.budgetSource === 'conservative'
          ? ' (conservative until calibrated)'
          : info.budgetSource === 'profile'
            ? " (this model's profile)"
            : '';
      return `${levels}${source}. The context left can lower it; a larger budget does not guarantee a better answer.`;
    }
    case 'level':
      return 'This model has three native reasoning levels (low, medium, high); PWR passes your choice to it. It is a level, not a token budget.';
    case 'off_by_profile':
      return 'Thinking is turned off for this model by its profile, so the setting has no effect.';
    case 'unsafe':
      return 'Calibration found that ending this model’s thinking early did not produce an answer, so no budget is enforced. PWR uses the model’s own defaults.';
    case 'none':
      return 'No separate reasoning phase was observed for this model. PWR uses compatible generation defaults.';
    case 'observable_only':
      return 'This backend shows the model’s reasoning but cannot bound it per request. PWR uses the model’s own defaults.';
    default:
      return 'This model does not expose a separately controllable reasoning phase that PWR recognizes. PWR uses compatible generation defaults; Quick Calibration can check.';
  }
}
