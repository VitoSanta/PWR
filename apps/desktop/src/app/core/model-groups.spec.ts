import { describe, expect, it } from 'vitest';
import { groupModels, modelIdentity } from './model-groups';

describe('model families', () => {
  it('groups quantizations and formats while retaining engine references', () => {
    const refs = [
      'mlx-community/gemma-4-26b-a4b-it-8bit',
      'other/gemma-4-26b-a4b-it-Q4_K_M.gguf',
      'mlx-community/gemma-4-26b-a4b-it-4bit',
    ];
    const groups = groupModels(refs, (ref) => ref);
    expect(groups).toHaveLength(1);
    expect(groups[0].name).toBe('Gemma 4 26B A4B it');
    expect(new Set(groups[0].variants)).toEqual(new Set(refs));
    expect(modelIdentity(refs[0]).variant).toBe('8-bit · MLX');
    expect(modelIdentity(refs[1]).variant).toBe('Q4_K_M · GGUF');
  });

  it('places QAT variants under their base model with distinct variant labels', () => {
    for (const size of ['e4b', '26b-a4b']) {
      const refs = [`mlx-community/gemma-4-${size}-it-4bit`, `mlx-community/gemma-4-${size}-it-qat-4bit`, `mlx-community/gemma-4-${size}-it-6bit`];
      const groups = groupModels(refs, ref => ref);
      expect(groups).toHaveLength(1);
      expect(new Set(groups[0].variants)).toEqual(new Set(refs));
      expect(modelIdentity(refs[1]).variant).toBe('QAT · 4-bit · MLX');
      expect(modelIdentity(refs[1]).key).toBe(modelIdentity(refs[0]).key);
    }
  });

  it('keeps sizes, tuning and assistant models separate', () => {
    const refs = [
      'gemma-4-26b-a4b-it-4bit',
      'gemma-4-31b-it-4bit',
      'gemma-4-26b-a4b-pt-4bit',
      'gemma-4-26b-a4b-it-assistant-bf16',
    ];
    expect(groupModels(refs, (ref) => ref)).toHaveLength(4);
  });

  it('orders families by base size with QAT variants kept inside their family', () => {
    const refs = ['31b', '26b-a4b', '12b', 'e4b', 'e2b', 'e4b-it-qat']
      .map(size => `mlx-community/gemma-4-${size}${size.endsWith('qat') ? '' : '-it'}-4bit`);
    const groups = groupModels(refs, ref => ref);
    expect(groups.map(group => group.name)).toEqual([
      'Gemma 4 E2B it', 'Gemma 4 E4B it', 'Gemma 4 12B it',
      'Gemma 4 26B A4B it', 'Gemma 4 31B it',
    ]);
    expect(groups[1].variants).toHaveLength(2);
  });

  it('supports unquantized models and explicit precision', () => {
    expect(modelIdentity('vendor/custom-model').variant).toBe('Default');
    expect(modelIdentity('gemma-4-e2b-it-bf16').variant).toBe('BF16');
    expect(modelIdentity('gemma-4-31b-it-MXFP4').variant).toBe('MXFP4');
  });
});
