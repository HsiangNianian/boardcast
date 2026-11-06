# Implemented shell: visual and keyboard matrix

Date: 2026-11-03. The production App was checked in an isolated Chromium profile with protocol-shaped native data: a populated meeting, long title, transcript, summary, graph, ready board model, and listening status. No audio was recorded and no real meeting files were changed.

## Matrix and reproducibility

All 24 combinations passed: Chinese/English × light/dark × the three physical windows below × 100%/200% browser zoom.

| Physical window | CSS viewport at 100% | CSS viewport at 200% |
| --- | --- | --- |
| 1280 × 800 | 1280 × 800 | 640 × 400 |
| 900 × 600 | 900 × 600 | 450 × 300 |
| 720 × 480 | 720 × 480 | 360 × 240 |

The runner uses `chrome.tabs.setZoom(2)` in a temporary extension and profile, then checks `devicePixelRatio === 2` and `visualViewport.scale === 1`. It does not substitute a small viewport, CSS zoom, or pinch zoom. Browser resize and the textarea's ResizeObserver are allowed to settle before measurements. Profiles and extension files are removed afterward.

```sh
node node_modules/vite/bin/vite.js build --outDir /tmp/boardcast-native-shell-dist
node node_modules/vite/bin/vite.js preview --outDir /tmp/boardcast-native-shell-dist --host 127.0.0.1 --port 1424
PLAYWRIGHT_PATH=/path/to/node_modules/playwright node scripts/ui/run-visual-matrix.cjs http://127.0.0.1:1424
```

Evidence: [machine-readable matrix](evidence/implementation/visual-matrix/matrix.json). Each case has board, contents, settings, and review screenshots in the same directory. The production output is separate from the shared development/build output, avoiding concurrent HMR or replaced lazy chunks.

The later export/save-time batch repeated all 24 cases on the frozen production preview at port 1430 and passed. Its separate [current matrix](evidence/implementation/export-visual-matrix/matrix.json) includes the revised compact header, short-window stop alignment, and focus-based sidebar navigation; see [the interaction report](export-interaction-validation.md). The runner now waits for the responsive layout to commit after browser zoom, keeps the original geometry assertions, and records full failure stacks.

## What the checks establish

The shell has no horizontal overflow, the content area retains nonzero usable height, and stop, settings close, and review cancel controls remain reachable. The smallest content viewport measured 65 CSS pixels. At the two shortest CSS heights, the header, details, and capture area scroll independently; long text uses the textarea's own scrolling. This is a constrained layout, not a claim that every control is simultaneously visible in 360 × 240 CSS pixels.

Each case exercises outer board/details tabs, inner transcript/summary/contents tabs, direction/Home/End navigation, contents page selection focus, all six settings categories, both dialog Tab directions, Escape and trigger focus restoration, and import menu Home/End/Escape. A long multiline draft and Shift+Enter remain editable without submitting; the stop control remains reachable afterward. Speakers and review are opened and canceled at each size, followed by another stop-control reachability check. Actual stop/save ordering is covered separately by [native bridge validation](native-shell-validation.md).

The initial matrix found eight failures at the two smaller 200% windows. Meeting actions and board icons did not wrap, and the generic progress-bar `.bar` rule imposed a fixed height and hidden overflow on the Composer. The repair gives the Composer its own automatic height, wraps narrow actions, reserves a content viewport at short heights, and moves action menus into a viewport-positioned portal so scrolling headers cannot clip them. Menu focus, outside click, Escape, and composition isolation are retained.

## Contrast and boundaries

`check-theme-contrast.cjs` reads actual rendered CSS colors for nine selected pairs in each theme: primary/secondary text, secondary text on the alternate surface, success/warning/error/accent text, control borders, and focus rings. All 18 pairs passed their 4.5:1 text or 3:1 non-text targets. Evidence: [rendered-theme-contrast.json](evidence/implementation/rendered-theme-contrast.json). This does not establish every hover, disabled, chart, canvas, or mixed-background combination, and is not a full WCAG conformance claim.

The matrix uses reduced motion for deterministic capture. Native macOS focus/responder behavior, VoiceOver reading order, real Chinese/Japanese IME delivery, microphone permissions/finalization, and touch hardware remain platform checks. The independent Preparation capability matrix and maintenance failure checks are documented separately and were not replaced by these screenshots.
