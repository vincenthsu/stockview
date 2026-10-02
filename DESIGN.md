# StockView design system ("chart paper")

World: hand-plotted chart paper. Pale mint graph-paper ground, ink-black text, one vermilion 0% baseline, a ranked "finish board" of end-labels at the right edge of the compare chart. Operate mode: standard controls, the world lends palette, type, density and one signature move only.

## Tokens (src/theme.rs)
- Light: ground #EEF3EA, panel #E5ECE0, grid #DAE4D5, grid-major #C6D3C1, hairline #B4C2AF, ink #1B2420, dim #56645A, baseline #D6402B.
- Night: ground #101713, panel #151D18, grid #1C2720, ink #E4EDE2, dim #8FA096, baseline #FF6B4F.
- Up/down: Taiwan red-up (#C8321F / #0F7A55); switchable to US green-up.
- Series inks (8): blue #2459C4, orange #D96F00, magenta #B5338A, teal #0B8479, ochre #8A6F00, violet #6C4BC2, rose #D02F4E, slate #4A5B63 (brighter variants at night). Assigned per symbol, persisted.
- Radius 2px, hairline 1px strokes, no cards, no shadows except popups.
- Type: egui proportional (Ubuntu) + system CJK fallback; Hack monospace for all numerals (tabular).

## Components
Chip buttons (ink fill when active), flush hairline-divided panels, watchlist rows with series swatch, stats table with proportional return bars, registration crosses at plot corners, dashed crosshair with value chips.

## Signature move
0% vermilion rule + ranked end-label chips with leader lines (src/compare.rs).
