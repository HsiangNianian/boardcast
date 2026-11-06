# B4 export content acceptance — 2026-11-03

The updated production browser run passed **68 checks, 0 failures, 0 script errors**, including truthful browser-download handoff wording. The authoritative artifact list and measured results are in [export-check.json](evidence/implementation/exports/export-check.json). This content run loaded `assets/index-DOK_F9Rp.js` from the production preview at `http://127.0.0.1:1430`; subsequent save-time and compact-layout changes are checked separately in the export interaction report.

## Reproduction and fixture

```sh
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-checks.cjs http://127.0.0.1:1430 check-exports.cjs
```

The check uses an existing Chromium/Playwright installation and installed `pdfinfo`/`pdftoppm`; it adds no dependency. It seeds an isolated browser context with three pages: a semantic diagram with Chinese/English labels, a cyan PNG background, a magenta free shape, free text/stroke, red PNG and green SVG pictures; a page containing only free objects; and an empty current page. Summary and two-speaker transcript markers identify meeting-wide content. A separate background-only meeting checks Markdown's omission notice. All files are downloaded through the actual UI.

## Evidence by format

| Format | Content verified |
| --- | --- |
| Markdown | Meeting title, summary, both transcript lines, semantic labels and all page headings. An empty current page does not block output. Free visuals on other pages and a background-only meeting both produce an explicit omission notice. |
| PNG | PAD and PC outputs contain the background, free shape and both image types, measured from decoded pixels. A graph-empty page with free content also produces visible pixels. |
| SVG | Embedded background and visible free/image pixels in both board modes. A graph-empty page remains exportable. |
| Animated SVG | Timeline metadata and background survive; natural playback over 14 seconds increases semantic dark pixels from 1,123 to 4,173. The final frame retains the free objects and images. |
| Excalidraw | Editable shape, stroke and text IDs; exact PNG/SVG data URLs; semantic graph and layout metadata; a portable visible background image. The free-only file remains editable with an empty graph. |
| Meeting PDF | Three output pages: two nonempty boards, then summary/transcript. Rasterized first page has cyan/magenta/red/green populations of 346,453 / 13,615 / 7,676 / 7,834 pixels; the free-only second page has 171,532 magenta pixels. The final page contains rendered summary/transcript text and was visually inspected. |
| Replay HTML | A new browser context opens the downloaded file with HTTP disabled. All three pages, summary, transcript, Chinese stroke data, backgrounds and free projections are embedded. Initial/final keyboard seeks restore the expected scenes and embedded images decode. There are zero HTTP attempts, request failures or script errors. |

The standalone player was also measured at 1280 and 720 pixels. At 1280 its board occupies the intended 920-pixel column beside a 360-pixel transcript; at 720 both span the viewport in separate rows, with no horizontal overflow. Screenshots include [final first board](evidence/implementation/exports/offline-final-board1.png), [free-only board](evidence/implementation/exports/offline-final-board2.png), and [compact player](evidence/implementation/exports/offline-compact.png).

## Concrete defects found and repaired

1. Markdown exported from an empty current page did not receive the meeting replay snapshot, hiding other pages' visual omissions. App now captures that snapshot for Markdown, and the exporter reports both free objects and backgrounds.
2. Newly exported HTML could store an uncomputed layout for a page never opened before export. The exporter now records the completed captured layout for every page; the lightweight offline player does not need the full layout engine.
3. The standalone player's board inherited workspace grid coordinates, creating a blank implicit column and clipping its header. Explicit player grid coordinates and compact stacking restore the intended canvas/log geometry.

## Boundaries

These are content and interaction checks against a controlled production fixture, not merely successful downloads. The PDF remains image-based; its text is not selectable. Markdown intentionally cannot represent arbitrary drawings or background images and states that omission. The checks do not establish all-language font coverage, native macOS save-dialog cancellation, filesystem permission behavior, or #68 summary factuality. Earlier artifacts are retained for comparison; use the latest JSON manifest to identify the passing outputs.
