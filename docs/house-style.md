# House style

StarSheets should feel like Google Sheets. These are the sizes, colours and shapes every part of
the app uses. They live in `app/src/house.rs`: build new UI from its helpers, never with numbers of
its own.

## Spacing

Everything sits on a 4 px scale.

| Name | Size | Used for |
|---|---|---|
| XS | 4 px | Between swatches, inside small bubbles |
| S | 8 px | Between items in a dialog, between buttons |
| M | 12 px | Inside text boxes, inside a note |
| L | 16 px | Between groups of fields, side panel and card padding, menu side padding |
| XL | 24 px | Dialog padding, the space above a dialog's buttons |

## Text

| Use | Size | Colour |
|---|---|---|
| Dialog title | 22 px | #1F1F1F |
| Field label | 14 px, bold | #1F1F1F |
| Body, menu rows, buttons | 14 px | #1F1F1F |
| Help text under a label | 12 px | #444746 |
| Small capitals heading (STANDARD) | 11 px, bold | #444746 |
| Menu shortcuts | 13 px | grey |
| Errors | 12 px | #B3261E |

## Shapes

| Thing | Corners | Padding | Other |
|---|---|---|---|
| Dialog | 8 px | 24 px | White, soft shadow, title at the top left, ✕ at the top right |
| Menu | 8 px | 8 px top and bottom | Rows run edge to edge, 32 px high, 16 px side padding; anything that isn't a row (a palette) is inset 16 px |
| Card (note, link, date picker) | 8 px | 16 px (notes 12 px) | White, soft shadow |
| Bubble (calculation preview) | 4 px | 8 x 4 px | |
| Side panel | none | 16 px | 1 px divider line |
| Text box, drop-down | 4 px | 12 px inside | 36 px high, 1 px grey outline (#747775), blue when open |
| Button | pill (18 px) | 12 px inside | 36 px high, at least 88 px wide |
| Divider | | 8 px above and below | 1 px #DADCE0 |

## Lists and drop-downs

- A drop-down (`house::select`) opens a menu that shows **every choice at once**, as tall as the
  window allows, with a ✓ by the current one and dividers between groups.
- When it still doesn't fit (a small window or a low-resolution screen), it scrolls with a
  **solid scroll bar** that is always visible, so it's obvious there is more. Every scrolling
  list uses solid bars; only the sheet tab strip keeps a thin bar that shows on hover.

## Buttons

- **Main button:** green (#146C2E) pill with white text. Only one per dialog, at the far right.
- **Other buttons:** white pill, grey outline, green text, to the left of the main one, 8 px apart.
- **Buttons inside a list** ("Restore"): green text, no outline.

## Selection and hover

| State | Look |
|---|---|
| Selected cells | Blue (#1A73E8) 2 px border, light blue fill |
| Menu row hover | #F1F3F4 |
| Selected item in a list or chip | #E8F0FE |
| Focused field | Blue outline |

## Live preview

Where a change can be shown before it is applied (colours, formats), show it in the sheet straight
away; Cancel puts it back.
