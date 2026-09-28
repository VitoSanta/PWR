#!/usr/bin/env python3
"""Colour themes for the desktop app, from a few base colours each.

    python3 tools/palettes.py

writes `src/styles/palettes.css` (the tokens, keyed on
<html data-palette>) and `src/app/core/palettes.ts` (what Settings lists and
draws). Every palette is checked for contrast before anything is written: body
text, secondary and muted text, the accent and state colours as text, and the
text on solid buttons, against the surfaces they sit on (WCAG 2.1: 4.5:1 for
text, 7:1 for primary text). A palette that fails is not written.

A palette only sets colours. Geometry -- radii, spacing, type -- stays the
layout's, so every layout can be drawn in every palette.
"""

import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent.parent


def rgb(hex_colour):
    h = hex_colour.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def rgba(hex_colour, alpha):
    r, g, b = rgb(hex_colour)
    return f'rgba({r}, {g}, {b}, {alpha})'


def mix(a, b, t):
    """`a` moved `t` of the way to `b`."""
    ra, rb = rgb(a), rgb(b)
    return '#' + ''.join(f'{round(x + (y - x) * t):02x}' for x, y in zip(ra, rb))


def luminance(hex_colour):
    def channel(c):
        c /= 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (channel(c) for c in rgb(hex_colour))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


# One entry per palette. `data` is the categorical series (charts, the graph,
# the context meter). Colours not given are derived in `tokens`.
PALETTES = [
    dict(
        id='code-dark', label='Code Dark', scheme='dark',
        description="The editor's modern dark: neutral greys, one blue",
        app='#181818', sidebar='#181818', surface='#1f1f1f', surface2='#252526',
        elevated='#252526', inset='#141414', input='#2a2a2a',
        text='#cccccc', text2='#a8a8a8', muted='#8f8f8f',
        accent='#0078d4', accent_text='#4daafc', on_accent='#ffffff',
        secondary='#4ec9b0',
        success='#3fb950', warning='#e2c08d', danger='#f85149', danger_solid='#c72e0f',
        data=['#4daafc', '#4ec9b0', '#c586c0', '#dcdcaa', '#ce9178', '#9d9d9d', '#d16969'],
        tint='#ffffff',
    ),
    dict(
        id='midnight', label='Midnight', scheme='dark',
        description='Deep indigo, soft blue and violet',
        app='#16161e', sidebar='#16161e', surface='#1a1b26', surface2='#1f2335',
        elevated='#24283b', inset='#13131a', input='#1f2030',
        text='#c0caf5', text2='#a9b1d6', muted='#8b92b8',
        accent='#7aa2f7', accent_text='#7aa2f7', on_accent='#16161e',
        secondary='#bb9af7',
        success='#9ece6a', warning='#e0af68', danger='#f7768e', danger_solid='#f7768e',
        on_danger='#16161e',
        data=['#7aa2f7', '#7dcfff', '#bb9af7', '#e0af68', '#9ece6a', '#565f89', '#f7768e'],
        tint='#c0caf5',
    ),
    dict(
        id='arctic', label='Arctic', scheme='dark',
        description='Polar night greys and frost blues',
        app='#242933', sidebar='#2b303b', surface='#2e3440', surface2='#3b4252',
        elevated='#3b4252', inset='#272c36', input='#2b303b',
        text='#eceff4', text2='#d8dee9', muted='#aab4c6',
        accent='#88c0d0', accent_text='#88c0d0', on_accent='#242933',
        secondary='#c3a3be',
        success='#a3be8c', warning='#ebcb8b', danger='#e5838c', danger_solid='#e5838c',
        on_danger='#242933',
        data=['#88c0d0', '#81a1c1', '#b48ead', '#ebcb8b', '#a3be8c', '#5e81ac', '#d08770'],
        tint='#eceff4',
    ),
    dict(
        id='ember', label='Ember', scheme='dark',
        description='Warm charcoal, amber and orange',
        app='#1d2021', sidebar='#1d2021', surface='#282828', surface2='#32302f',
        elevated='#3c3836', inset='#1a1c1d', input='#232526',
        text='#ebdbb2', text2='#d5c4a1', muted='#b3a38f',
        accent='#fe8019', accent_text='#fe8019', on_accent='#1d2021',
        secondary='#8ec07c',
        success='#b8bb26', warning='#fabd2f', danger='#fd6150', danger_solid='#fd6150',
        on_danger='#1d2021',
        data=['#fe8019', '#8ec07c', '#d3869b', '#fabd2f', '#b8bb26', '#928374', '#83a598'],
        tint='#ebdbb2',
    ),
    dict(
        id='contrast-dark', label='High Contrast', scheme='dark',
        description='Black, white and hard edges, for legibility first',
        app='#000000', sidebar='#000000', surface='#000000', surface2='#0b0b0b',
        elevated='#0a0a0a', inset='#000000', input='#000000',
        text='#ffffff', text2='#f0f0f0', muted='#d6d6d6',
        accent='#6fc3df', accent_text='#6fc3df', on_accent='#000000',
        secondary='#f38518',
        success='#89d185', warning='#ffd370', danger='#f48771', danger_solid='#f48771',
        on_danger='#000000',
        data=['#6fc3df', '#f38518', '#c586c0', '#ffd370', '#89d185', '#d6d6d6', '#f48771'],
        tint='#ffffff',
        border_subtle='rgba(111, 195, 223, 0.6)', border_strong='#6fc3df', focus='#f38518',
    ),
    dict(
        id='code-light', label='Code Light', scheme='light',
        description="The editor's modern light: white, grey and a deep blue",
        app='#f8f8f8', sidebar='#f8f8f8', surface='#ffffff', surface2='#f3f3f3',
        elevated='#ffffff', inset='#f3f3f3', input='#ffffff',
        text='#3b3b3b', text2='#5c5c5c', muted='#6b6b6b',
        accent='#005fb8', accent_text='#005fb8', on_accent='#ffffff',
        secondary='#267f99',
        success='#1a7f37', warning='#895503', danger='#c72e0f', danger_solid='#c72e0f',
        data=['#0070c1', '#267f99', '#af00db', '#795e26', '#098658', '#6b6b6b', '#a31515'],
        tint='#1f1f1f',
    ),
    dict(
        id='solar', label='Solar', scheme='light',
        description='Warm cream with the classic blues and cyans',
        app='#eee8d5', sidebar='#eee8d5', surface='#fdf6e3', surface2='#f5efdc',
        elevated='#fdf6e3', inset='#f5efdc', input='#fffbef',
        text='#073642', text2='#4a5f66', muted='#53646a',
        accent='#1c6aa5', accent_text='#1c6aa5', on_accent='#ffffff',
        secondary='#1d766f',
        success='#5f6e00', warning='#8a6700', danger='#b92b28', danger_solid='#b92b28',
        data=['#268bd2', '#2aa198', '#6c71c4', '#b58900', '#859900', '#93a1a1', '#d33682'],
        tint='#073642',
    ),
    dict(
        id='linen', label='Linen', scheme='light',
        description='Paper-warm neutrals and terracotta',
        app='#f4efe6', sidebar='#efe8dc', surface='#fbf8f2', surface2='#f4efe6',
        elevated='#fffdf9', inset='#f1ebe0', input='#fffdf9',
        text='#2b2620', text2='#554c42', muted='#675d51',
        accent='#a3481f', accent_text='#a3481f', on_accent='#ffffff',
        secondary='#35695c',
        success='#2f6f45', warning='#865c00', danger='#a8332a', danger_solid='#a8332a',
        data=['#b4532a', '#3d7a6b', '#7d5ba6', '#b8862b', '#4f8a3c', '#8c8175', '#b04f7c'],
        tint='#2b2620',
    ),
    dict(
        id='contrast-light', label='High Contrast Light', scheme='light',
        description='White, black and hard edges, for legibility first',
        app='#ffffff', sidebar='#ffffff', surface='#ffffff', surface2='#f7f7f7',
        elevated='#ffffff', inset='#ffffff', input='#ffffff',
        text='#000000', text2='#1a1a1a', muted='#333333',
        accent='#0f4a85', accent_text='#0f4a85', on_accent='#ffffff',
        secondary='#b5200d',
        success='#006e1a', warning='#6b4f00', danger='#b5200d', danger_solid='#b5200d',
        data=['#0f4a85', '#b5200d', '#7a1fa2', '#6b4f00', '#006e1a', '#333333', '#a01461'],
        tint='#000000',
        border_subtle='rgba(15, 74, 133, 0.55)', border_strong='#0f4a85', focus='#b5200d',
    ),
]


def tokens(p):
    dark = p['scheme'] == 'dark'
    toward = '#ffffff' if dark else '#000000'
    solid_hover = mix(p['accent'], toward, 0.12)
    danger_hover = mix(p['danger_solid'], toward, 0.12)
    shadow = '#000000' if dark else mix(p['text'], '#000000', 0.2)
    t = {
        'bg-app': p['app'],
        'bg-sidebar': p['sidebar'],
        'surface-primary': p['surface'],
        'surface-secondary': p['surface2'],
        'surface-elevated': p['elevated'],
        'surface-inset': p['inset'],
        'surface-hover': rgba(p['tint'], 0.07 if dark else 0.05),
        'surface-active': rgba(p['tint'], 0.12 if dark else 0.09),
        'scrim': rgba(mix(p['app'], '#000000', 0.5), 0.62 if dark else 0.32),
        'input-bg': p['input'],
        'tooltip-bg': mix(p['elevated'], p['text'], 0.12) if dark else mix(p['text'], '#000000', 0.1),
        'tooltip-text': p['text'] if dark else p['surface'],
        'border-subtle': p.get('border_subtle', rgba(p['tint'], 0.12 if dark else 0.11)),
        'border-strong': p.get('border_strong', rgba(p['tint'], 0.22 if dark else 0.2)),
        'text-primary': p['text'],
        'text-secondary': p['text2'],
        'text-muted': p['muted'],
        'text-on-accent': p['on_accent'],
        'text-on-danger': p.get('on_danger', '#ffffff'),
        'accent-primary': p['accent'],
        'accent-text': p['accent_text'],
        'accent-solid': p['accent'],
        'accent-solid-hover': solid_hover,
        'accent-soft': rgba(p['accent'], 0.16 if dark else 0.1),
        'accent-border': rgba(p['accent'], 0.45 if dark else 0.38),
        'accent-secondary': p['secondary'],
        'accent-secondary-soft': rgba(p['secondary'], 0.14 if dark else 0.1),
        'accent-secondary-text': p['secondary'],
        'brand-gradient': f"linear-gradient(135deg, {p['accent']} 0%, {mix(p['accent'], p['secondary'], 0.5)} 50%, {p['secondary']} 100%)",
        'focus-ring': p.get('focus', rgba(p['accent'], 0.75 if dark else 0.6)),
        'success': p['success'],
        'success-text': p['success'],
        'success-soft': rgba(p['success'], 0.13 if dark else 0.1),
        'success-border': rgba(p['success'], 0.38 if dark else 0.32),
        'warning': p['warning'],
        'warning-text': p['warning'],
        'warning-soft': rgba(p['warning'], 0.13 if dark else 0.1),
        'warning-border': rgba(p['warning'], 0.4 if dark else 0.36),
        'danger': p['danger'],
        'danger-text': p['danger'],
        'danger-soft': rgba(p['danger'], 0.13 if dark else 0.08),
        'danger-border': rgba(p['danger'], 0.4 if dark else 0.34),
        'danger-solid': p['danger_solid'],
        'danger-solid-hover': danger_hover,
        'diff-add-bg': rgba(p['success'], 0.1),
        'diff-add-text': mix(p['success'], p['text'], 0.45),
        'diff-del-bg': rgba(p['danger'], 0.1),
        'diff-del-text': mix(p['danger'], p['text'], 0.45),
        'shadow-sm': f"0 1px 2px {rgba(shadow, 0.3 if dark else 0.06)}",
        'shadow-popover': f"0 12px 32px {rgba(shadow, 0.5 if dark else 0.12)}, 0 2px 6px {rgba(shadow, 0.3 if dark else 0.06)}",
        'shadow-modal': f"0 24px 64px {rgba(shadow, 0.6 if dark else 0.18)}, 0 4px 12px {rgba(shadow, 0.3 if dark else 0.08)}",
        'shadow-overlay-panel': f"0 0 40px {rgba(shadow, 0.45 if dark else 0.14)}",
    }
    for i, colour in enumerate(p['data'], 1):
        t[f'data-{i}'] = colour
    return t


def check(p):
    """(label, ratio, minimum) for every pairing a person reads."""
    s, s2 = p['surface'], p['surface2']
    pairs = [
        ('text on surface', p['text'], s, 7),
        ('text on app', p['text'], p['app'], 7),
        ('secondary on surface', p['text2'], s, 4.5),
        ('secondary on surface-2', p['text2'], s2, 4.5),
        ('muted on surface', p['muted'], s, 4.5),
        ('muted on sidebar', p['muted'], p['sidebar'], 4.5),
        ('accent text on surface', p['accent_text'], s, 4.5),
        ('on-accent on accent', p['on_accent'], p['accent'], 4.5),
        ('on-danger on danger', p.get('on_danger', '#ffffff'), p['danger_solid'], 4.5),
        ('secondary accent on surface', p['secondary'], s, 4.5),
        ('success on surface', p['success'], s, 4.5),
        ('warning on surface', p['warning'], s, 4.5),
        ('danger on surface', p['danger'], s, 4.5),
    ]
    return [(label, contrast(a, b), minimum) for label, a, b, minimum in pairs]


SWATCH_TOKENS = {
    'sidebar': 'bg-sidebar', 'app': 'bg-app', 'surface': 'surface-primary',
    'accent': 'accent-primary', 'text': 'text-primary', 'secondary': 'accent-secondary',
}


def declarations(block):
    return dict(re.findall(r'--([\w-]+):\s*([^;]+);', block))


def layout_swatches():
    """Each layout's own colours in each scheme -- what Settings draws for
    "Layout colours" -- read from tokens.css (studio, the base) and the
    layouts' blocks in variants.css."""
    tokens_css = (HERE / 'src/styles/tokens.css').read_text()
    base = {
        scheme: declarations(tokens_css.split(f":root[data-theme='{scheme}'] {{", 1)[1].split('}', 1)[0])
        for scheme in ('dark', 'light')
    }
    found = {'studio': {scheme: {k: base[scheme][t] for k, t in SWATCH_TOKENS.items()} for scheme in base}}
    variants_css = (HERE / 'src/styles/variants.css').read_text()
    for layout, scheme, block in re.findall(
        r":root\[data-variant='(\w+)'\]\[data-theme='(\w+)'\] \{(.*?)\n\}", variants_css, re.S
    ):
        own = {**base[scheme], **declarations(block)}
        found.setdefault(layout, {})[scheme] = {k: own[t] for k, t in SWATCH_TOKENS.items()}
    for layout, schemes in found.items():
        for scheme, swatch in schemes.items():
            for key, value in swatch.items():
                if not re.fullmatch(r'#[0-9a-f]{6}', value):
                    sys.exit(f'{layout} {scheme}: --{SWATCH_TOKENS[key]} is {value!r}, not #rrggbb')
    return found


def swatch_ts(s):
    return (
        f"{{ sidebar: '{s['sidebar']}', app: '{s['app']}', surface: '{s['surface']}', "
        f"accent: '{s['accent']}', text: '{s['text']}', secondary: '{s['secondary']}' }}"
    )


def main():
    failures = []
    for p in PALETTES:
        for label, ratio, minimum in check(p):
            if ratio < minimum:
                failures.append(f"{p['id']}: {label} {ratio:.2f} < {minimum}")
    if failures:
        sys.exit('contrast below WCAG 2.1:\n  ' + '\n  '.join(failures))

    css = [
        '/*',
        ' * Colour themes. Generated by tools/palettes.py -- edit that, not this.',
        ' * A palette sets colours only, so every layout can be drawn in every',
        ' * palette; it wins over a layout\'s own colours (this file comes last).',
        ' * No palette set means the layout\'s own colours.',
        ' */',
        '',
    ]
    for p in PALETTES:
        css.append(f":root[data-palette='{p['id']}'][data-theme] {{")
        css.append(f"  color-scheme: {p['scheme']};")
        for name, value in tokens(p).items():
            css.append(f'  --{name}: {value};')
        css.append('}')
        css.append('')
    (HERE / 'src/styles/palettes.css').write_text('\n'.join(css))

    entries = []
    for p in PALETTES:
        entries.append(
            f"  {{ id: '{p['id']}', label: '{p['label']}', scheme: '{p['scheme']}', "
            f"description: {p['description']!r}, swatch: {swatch_ts(p)} }},"
        )
    layouts = layout_swatches()
    ts = f"""// Generated by tools/palettes.py -- edit that, not this.

export type Scheme = 'light' | 'dark';

export interface Swatch {{
  sidebar: string;
  app: string;
  surface: string;
  accent: string;
  text: string;
  secondary: string;
}}

export interface PaletteInfo {{
  id: string;
  label: string;
  scheme: Scheme;
  description: string;
  /** What Settings draws for it, before it is applied. */
  swatch: Swatch;
}}

export const PALETTES: PaletteInfo[] = [
{chr(10).join(entries)}
];

/** Each layout's own colours, drawn for "Layout colours" (tokens.css and variants.css). */
export const LAYOUT_SWATCHES: Record<string, Record<Scheme, Swatch>> = {{
{chr(10).join(f"  {layout}: {{ dark: {swatch_ts(sw['dark'])}, light: {swatch_ts(sw['light'])} }}," for layout, sw in layouts.items())}
}};
"""
    (HERE / 'src/app/core/palettes.ts').write_text(ts)
    for p in PALETTES:
        worst = min(check(p), key=lambda item: item[1] / item[2])
        print(f"{p['id']:15} ok  (tightest: {worst[0]} {worst[1]:.2f}:1)")


if __name__ == '__main__':
    main()
