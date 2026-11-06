# Topic link routing and first editor render — 2026-11-03

The controlled production fixture passed **10 browser checks with no script errors**, using `assets/index-CFFeOTzi.js` on the isolated preview at `http://127.0.0.1:1432`. The [manifest and measured geometry](evidence/implementation/topic-links/topic-link-check.json) identify the downloaded PNG/SVG/PDF/Excalidraw artifacts. Run `check-topic-links.cjs` through the documented UI runner.

## Root cause and repair

The topic anchor described the whole region, including its child item. Center-to-center clipping therefore placed the explicit edge's start inside the child text: the fixture's arrow started at `(220.26, 60.01)` while the child row covered `y=46.01…74.01`. No near-line label candidate was clear, so the fallback label and its paper backing also covered that row.

Only explicit links within one topic now reserve an outside lane in that topic's size. `Layout` supplies the same points and label bounds to hand-drawn composition, Excalidraw generation and edge hit testing. The topic title uses its actual text anchor when these links are present. The graph, parent relation, edge ID, generated label ID and both Excalidraw endpoint bindings remain intact. Diagram/ELK routing is unchanged. The reserved lane also contributes to export and viewport bounds.

The exact English and Chinese geometry regression verifies that every sampled arrow segment and the label avoid both node text boxes, the label is inside the exported bounds, the visible route can be selected, restoring layout memory reproduces it, and the input graph is unchanged. The full Node suite passed **242 tests, 0 failures** after the routing changes; TypeScript and `git diff --check` passed after the final font/initialization integration.

## First-render font and interaction failures found during visual inspection

The first PC screenshot exposed a separate clipping defect even though label-separation assertions passed. Excalidraw measured generated text before its font loaded; subsequent font loading invalidated drawing caches without refreshing element width. The initial title/item widths were 317/283, causing visible clipping. Generation now prepares the required font subsets through Excalidraw's public canvas export path with a one-pixel canvas, then converts the actual scene. Fonts already loaded bypass preparation.

The final cold-context measurements match the saved element dimensions exactly:

| Element | Saved width | Actual font measurement |
| --- | ---: | ---: |
| Topic title | 378 | 378 |
| Child item | 324 | 324 |
| Edge label | 88.84 | 88.84 |

Preparation has a five-second limit. A failure uses local Helvetica for newly generated text, with a 30-second retry backoff; already loaded fonts can be used immediately. A controlled font request that never resolved produced a complete, interactive fallback scene in **5,641 ms**, rather than leaving the board blank.

Waiting for fonts also exposed an initialization race: the editor could accept a first gesture before the app had applied its initial scene, and that scene then overwrote the gesture. The editor is now inert while `.pencil-stage[data-ready="false"]` is displayed. The existing scene-application completion switches it to `data-ready="true"`. The fault test verifies the inert gate and then draws a free rectangle immediately after readiness; that first shape persists successfully.

## Visual artifacts and boundaries

Inspected screenshots show complete, separate text in [hand-drawn mode](evidence/implementation/topic-links/hand-drawn.png), [free edit](evidence/implementation/topic-links/free-edit.png), [SVG export](evidence/implementation/topic-links/exported-svg.png), [PDF board raster](evidence/implementation/topic-links/pdf-board.png), and [the font timeout fallback](evidence/implementation/topic-links/font-timeout-fallback.png). The SVG assertion contains every actual path inside its viewBox. The editable export preserves both semantic arrow bindings and a bound label outside the text elements.

These checks cover the reported parent-topic/item fixture, its Chinese geometry equivalent and the deliberately stalled font request in Chromium. They do not claim a general obstacle router for arbitrary free drawings or native macOS font/network behavior. The library still owns its usual curved rendering in free edit; the shared route controls its endpoints and outside lane.
