# PWR brand palette

Shared identity tokens live in `apps/desktop/src/styles/tokens.css`.

| Token | Colour | Role |
| --- | --- | --- |
| `--brand-azure` | `#4C8DF6` | Leading identity colour, from the logo gradient |
| `--brand-blue` | `#1A73E8` | Main blue, logo stem and gradient start |
| `--brand-sky` | `#A8C7FA` | Light blue, gradient end |
| `--brand-spectrum` | Blue → azure → sky → blue | Continuous decorative spectrum |

Use these tokens for brand illumination and future identity accents throughout the design system. Do not duplicate hex values in components. The busy message composer is the first application: a ring in the three logo blues with a matching halo in both themes. Reduced motion keeps the illumination static.

These are decorative identity colours. Use semantic tokens for text, focus and status feedback; verify contrast when applying the palette to new components. Existing Material surfaces and semantic colours remain in place in this targeted change. Their migration is a separate design-system step.

## Rounded application controls — 2026-10-19

All application forms use the shared `.input` primitive (20px control radius),
including profile fields and multiline text areas. Select triggers use the same
radius and heights. Do not reintroduce 4px outlined fields beside rounded buttons.
Menus and select lists share the surface-container colour, level-2 elevation,
16px surface radius, and 20px rounded rows. Larger dialogs retain the 28px panel
shape from the same family. Small code/keycap/checkbox marks are not form fields.

Use minimum row heights and vertical padding rather than fixed heights when a
menu item includes a subtitle. The Options menu must fit both lines of Goal &
approvals. Run controls use stable descriptions, so toggling a permission never
changes their content height. Permission state remains exposed by aria-pressed.

When the app is in native full screen, a maximized/page tool hides conversation
navigation from pointer and keyboard use until restored or closed. In a Mac
window the conversation control remains alongside the traffic lights. Reserve
only the native titlebar clearance, not an additional strip for chat navigation.
Tool titles always toggle collapse, including the only card. Collapsing a
maximized card exits maximization and folds it in the same action; it never
automatically unfolds another card.

The left chevron mirrors the right-chevron glyph already bundled locally.
New symbols must be checked against the shipped subset, not merely added to
the icon-name map.
