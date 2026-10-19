/** Presentation only: keep the original reference intact for engine operations. */
export interface ModelIdentity {
  key: string;
  name: string;
  variant: string;
}

export function modelIdentity(ref: string): ModelIdentity {
  const file = ref.split(/[\\/]/).pop() || ref;
  const format = /\.gguf$|(?:^|[-_])gguf(?:$|[-_])/i.test(file)
    ? 'GGUF'
    : /(?:^|[-_])mlx(?:$|[-_])/i.test(file) || /^mlx-community\//i.test(ref)
      ? 'MLX'
      : '';
  const quantPattern =
    /(?:^|[-_])((?:\d+(?:\.\d+)?[-_]?bits?)|(?:mxfp\d+)|(?:[bfu]?fp\d+)|(?:bf\d+)|(?:[iq]\d+(?:_[a-z0-9]+)*))(?=$|[-_.])/i;
  const quant = quantPattern.exec(file);
  const precision = quant?.[1]
    .replace(/[-_]?bits?$/i, '-bit')
    .toUpperCase()
    .replace('-BIT', '-bit');
  const qat = /(?:^|[-_])qat(?=$|[-_.])/i.test(file);
  const base = file
    .replace(/\.gguf$/i, '')
    .replace(quantPattern, '')
    .replace(/(?:^|[-_])qat(?=$|[-_.])/gi, '')
    .replace(/(?:^|[-_])(?:mlx|gguf)(?=$|[-_])/gi, '')
    .replace(/[-_]+/g, ' ')
    .trim();
  const name = base
    .replace(/\bgemma\b/gi, 'Gemma')
    .replace(/\b(e?\d+b|a\d+b)\b/gi, (part) => part.toUpperCase());
  return {
    key: base.toLowerCase(),
    name,
    variant: [qat ? 'QAT' : '', precision, format].filter(Boolean).join(' · ') || 'Default',
  };
}

export function groupModels<T>(items: readonly T[], reference: (item: T) => string) {
  const groups = new Map<string, { key: string; name: string; variants: T[] }>();
  for (const item of items) {
    const identity = modelIdentity(reference(item));
    const group = groups.get(identity.key) ?? {
      key: identity.key,
      name: identity.name,
      variants: [],
    };
    group.variants.push(item);
    groups.set(identity.key, group);
  }
  const compare = (a: string, b: string) =>
    a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
  return [...groups.values()]
    .sort((a, b) => modelSize(a.name) - modelSize(b.name) || compare(a.name, b.name))
    .map((group) => ({
      ...group,
      variants: group.variants.sort(
        (a, b) =>
          compare(modelIdentity(reference(a)).variant, modelIdentity(reference(b)).variant) ||
          compare(reference(a), reference(b)),
      ),
    }));
}

/** Order by the base size, not the activated A4B suffix or quantization. */
function modelSize(name: string): number {
  const size = /\bE?(\d+(?:\.\d+)?)B\b/i.exec(name);
  return size ? Number(size[1]) : Number.POSITIVE_INFINITY;
}
