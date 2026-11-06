---
name: board-layout
description: How the whiteboard agent should choose operations so that the deterministic layout produces a readable board. Use when writing board operations from meeting speech, or when acting on a layout check hint.
---

# Board layout for the whiteboard agent

You never choose coordinates. The layout engine places everything from the structure you create, so
layout quality is decided by which operations you emit. Keep the structure clean and the board stays
readable.

## Choose the structure

| What was said | Use | Layout result |
| --- | --- | --- |
| Steps, calls, data flow, cause and effect | `E` between named things | Boxes and arrows, left to right |
| Lists, owners, options, to-dos | `T` then `L` | A titled column with numbered rows |
| Undecided or asked | `Q` | Red row beside its host, or in the open list |
| Extra detail about something | `K` | Small blue row below it |

## Rules that protect the layout

1. **Never connect a topic.** A topic is a heading with a column under it, not a box in a flow. Use
   `E` between concrete nodes. A topic used in an edge produces a long curve across the board.
2. **One arrow per pair.** For a request and its reply, draw one `E` and put both in the label. A
   pair of opposite arrows takes two lanes and hides the labels.
3. **Keep a topic to seven rows.** If a list grows past that, start another `T`. Long columns force
   the whole board to shrink.
4. **Names under 12 characters.** Width comes from the longest name in a column, so one long name
   pushes everything aside. Put the rest in `K`.
5. **Do not create empty topics.** Emit `T` together with its first `L`.
6. **Drawing instructions are not content.** "Draw an arrow here" is an instruction: connect the two
   things it refers to. Never write the words themselves on the board.
7. **Use marks, not rewrites.** To emphasize or reject something, use `M` on the existing id.

## Layout hints

A line starting with `排版提示` or `Layout hint` reports a problem on the current board. Fix it in
this turn only if the new line is related. Otherwise keep working, and do not repeat earlier output.

Hints from the checker:

| Code | Means | Do |
| --- | --- | --- |
| `topic_in_flow` | An edge touches a topic | `X` it and connect concrete nodes |
| `two_way` | Opposite arrows between a pair | `X` one and label the other |
| `crowded_topic` | Over the row limit | New rows go under a new `T` |
| `long_label` | A name is too wide | `R` to a shorter name, `K` for the rest |
| `empty_topic` | A topic has no rows | Add `L`, or `D` it |
| `board_too_large` | The board no longer fits legibly | Keep additions small; suggest a new page |

Layout defects such as overlaps are fixed automatically by re-laying out the board. Do not try to
correct them with operations.
