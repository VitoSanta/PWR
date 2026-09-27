import { VARIANTS, VARIANT_KEY, readVariant, variantInfo } from './variant';

describe('variant', () => {
  it('defaults to Studio', () => {
    expect(readVariant(undefined)).toBe('studio');
    expect(readVariant({ getItem: () => null })).toBe('studio');
    expect(readVariant({ getItem: () => 'brutalist' })).toBe('studio');
  });

  it('reads a saved choice', () => {
    expect(readVariant({ getItem: (key) => (key === VARIANT_KEY ? 'paper' : null) })).toBe('paper');
  });

  it('lets ?variant= override the saved choice, and ignores an unknown one', () => {
    const saved = { getItem: () => 'paper' };
    expect(readVariant(saved, '?demo&variant=mission')).toBe('mission');
    expect(readVariant(saved, '?variant=nope')).toBe('paper');
  });

  it('survives storage that throws', () => {
    expect(
      readVariant({
        getItem: () => {
          throw new Error('denied');
        },
      }),
    ).toBe('studio');
  });

  it('knows which layouts keep a sidebar, so ⌘B can open the switcher instead', () => {
    expect(VARIANTS.filter((variant) => variant.sidebar).map((variant) => variant.id)).toEqual(['studio', 'islands']);
    expect(variantInfo('focus').sidebar).toBe(false);
  });
});
