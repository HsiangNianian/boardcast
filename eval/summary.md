# Summary checks

`boardcast-tools summary-eval <root> [--llm <url> | --deepseek] [--repeat N]` writes a summary for each short
meeting in `summary-cases.json` and checks how sure each section is (#68): a guess stays an open question,
an answered question leaves them, a maybe-answer does not, a withdrawn plan is no decision, a lettered
speaker owns nothing, and something done there and then is no to-do.

2026-11-11, Qwen3.5-4B Q4_K_M on llama-server, 3 runs of each case:

| Case | Before | After |
| --- | ---: | ---: |
| zh-guess-stays-open | 2/3 | 3/3 |
| zh-answered-question-closes | 3/3 | 3/3 |
| zh-tentative-answer-stays-open | 3/3 | 2/3 |
| zh-withdrawn-plan | 0/3 | 2/3 |
| zh-letter-is-no-owner | 3/3 | 3/3 |
| zh-in-the-moment-is-no-todo | 3/3 | 2/3 |
| en-guess-stays-open | 3/3 | 3/3 |
| en-answered-question-closes | 0/3 | 2/3 |
| **All** | **17/24** | **20/24** |

"After" adds to the prompt that an answered and accepted question leaves Open questions, a maybe-answer
keeps it there, and an agreed choice is a Decision; and drops "(owner: none)" left on a to-do. The 4B model
varies from run to run (one case passed 0/3 in one full run and 3/3 alone), so small differences mean
little. Not yet run on DeepSeek.
