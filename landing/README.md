# boardcast website

A standalone static page, separate from the desktop app. It is English only. Its dependencies
are the ones already installed at the repository root, so there is nothing more to install.

```bash
pnpm site:dev       # http://localhost:1430
pnpm site:build     # → landing/dist/, with relative paths, so it opens under any subpath
pnpm site:preview
```

GitHub Pages serves this page at the root; `pages.yml` builds and publishes it.

## The idea

The page is one meeting, told as a set of drawing sheets. A thin frame with rulers runs around it, and a title block
in the corner shows the sheet in view, its title, the version and a running meeting clock.

1. **talk** — a line is said first, then a pen writes "talk, and the board draws itself." in stroke order, with its
   tip visible, and circles "itself" in red. The written words can be circled with the mouse: circling one goes to
   its sheet, as circling works in the app. "hear it again" wipes the sheet with an eraser and writes it again.
2. **four lines** — the meeting as four lines on one timeline: voice, voices, board and marks. A cursor walks it
   until the reader points, and shows what was said at that moment.
3. **the app** — the app window pins while five chapters scroll: listen, draw, correct, circle, replay. The board
   is drawn from the scroll position, so scrolling back erases it. Once the window pins and the page is still, the
   chapters play by themselves at a reading pace; any wheel, touch, key or click hands control back.
4. **during** — cards that each show one thing the app does.
5. **after** — what a meeting leaves, as pictograms made of dots that scatter when pointed at.
6. **specs** and downloads.
7. **now** — the reader's own time.

Reduced motion shows every drawing finished and turns off the autoplay.

## Files

| File | What it does |
|---|---|
| `src/pen.ts` | Writes text in stroke order with a visible pen tip, from the EMS Tech single-stroke font |
| `src/hero.ts` | The first sheet: the said line, the handwriting, circle targets and the eraser |
| `src/sheet.ts` | The rulers, the title block that follows the section, and the closing clock |
| `src/lasso.ts` | Circling anything on the page with the mouse or a pen |
| `src/signals.ts` | The four-line timeline and its cursor |
| `src/story.ts` | The pinned app, its board drawn from scroll progress, and the autoplay |
| `src/cards.ts`, `src/dots.ts` | The "during" cards and the dot-matrix pictograms |
| `src/pict.ts` | Pictograms made of blocks and lines, which assemble as they come into view |
| `src/meeting.ts` | The sample meeting: who said what, and when |

## Fonts

DM Sans and JetBrains Mono come from `@fontsource-variable`. The board's handwriting is an unmodified Latin subset of
Excalifont (`src/fonts/excalifont-latin.woff2`), the source of the app's English strokes. The pen in the first sheet
writes with EMS Tech from hersheytext (`src/emstech.json`). Both are under the SIL OFL; their licenses are in
`public/licenses/`.
