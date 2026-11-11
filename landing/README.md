# boardcast website

A standalone static page, separate from the desktop app. It is English only. The page and its styles are here;
what fills and animates them is Rust compiled to WebAssembly, in `crates/site`. [Trunk](https://trunkrs.dev) builds
both, and fetches the matching wasm-bindgen itself.

```bash
rustup target add wasm32-unknown-unknown   # or your distribution's Rust WebAssembly package
cd landing
trunk serve            # http://localhost:1430, rebuilt when landing/ or crates/site changes
trunk build --release  # → landing/dist/, with relative paths, so it opens under any subpath
```

The web demo under `/demo/` is a page of its own. To open it locally, run a second server for it; the first passes
`/demo/` on to it:

```bash
cd landing/demo
trunk serve            # on port 1431, behind http://localhost:1430/demo/
```

GitHub Pages serves this page at the root; `pages.yml` builds it on pull requests and publishes it from main.

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
| `index.html`, `style.css` | The page and its look |
| `crates/site/src/pen.rs` | Writes text in stroke order with a visible pen tip, along the app's Excalifont centerlines |
| `crates/site/src/hero.rs` | The first sheet: the said line, the handwriting, circle targets and the eraser |
| `crates/site/src/sheet.rs` | The rulers, the title block that follows the section, and the closing clock |
| `crates/site/src/lasso.rs` | Circling anything on the page with the mouse or a pen |
| `crates/site/src/signals.rs` | The four-line timeline and its cursor |
| `crates/site/src/story.rs` | The pinned app, its board drawn from scroll progress, and the autoplay |
| `crates/site/src/cards.rs`, `dots.rs` | The "during" cards and the dot-matrix pictograms |
| `crates/site/src/pict.rs` | Pictograms made of blocks and lines, which assemble as they come into view |
| `crates/site/src/meeting.rs` | The sample meeting: who said what, and when |

## Other pages

`demo/` is the web demo, built from `crates/demo`: the app's own board code replaying the sample meetings. `faq/`,
`privacy/` and `terms/` are plain pages styled by `pages.css`. The Rust crates listed in the terms are generated:
run `python3 scripts/site-licenses.py` after dependencies change.

## Fonts

DM Sans and JetBrains Mono are the Latin and Latin Extended files from Fontsource, in `fonts/`. The board's
handwriting is an unmodified Latin subset of Excalifont (`fonts/excalifont-latin.woff2`), the source of the app's
English strokes. The pen in the first sheet writes along those same strokes, the ASCII part of them
(`crates/site/src/excalifont.json`), which `scripts/excalifont-strokes.py` writes next to the app's. All three are
under the SIL OFL; their licenses are in `public/licenses/`.
