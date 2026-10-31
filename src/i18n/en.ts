// UI strings. English is the base dictionary and the fallback language; every other language is
// typed as `Messages`, so a missing key fails typecheck.
// Strings with parameters are functions: word order and punctuation belong to each language,
// never to the call site.

import type { Mark } from '../engine/graph.ts'

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`

export const en = {
  lang: {
    /** Written in both languages so it can be found from the wrong one */
    title: '语言 · Language',
    system: 'System default',
    hint: 'Language of the interface. Board and summary follow the language spoken in the meeting, whatever you pick here.',
  },

  theme: {
    system: 'System',
    light: 'Light',
    dark: 'Dark',
    aria: (name: string) => `Theme: ${name}, click to switch`,
    title: (name: string) => `Theme: ${name}`,
  },

  app: {
    untitled: 'Untitled meeting',

    // —— Model ——
    builtinLabel: 'Built-in',
    localLabel: 'Local',
    remoteLabel: (model: string) => `Remote ${model}`,
    builtinHttp: (status: number, text: string) => `Built-in model ${status}: ${text}`,
    checkingRemote: 'Checking remote model…',
    builtinReady: (name: string, warming: boolean) => `Built-in · ${name}${warming ? ' · warming up' : ''}`,
    builtinIdle: (name: string) => `Built-in · ${name} · standby`,
    builtinStarting: (name?: string) => `Starting ${name ?? 'built-in model'}…`,
    builtinUnavailable: 'Built-in model unavailable',
    noModel: 'No model connected',

    // —— Top bar ——
    meetingsTitle: 'Meetings: open, rename, delete',
    statusTitle: 'The model drawing the board — click for settings',
    cloud: 'Cloud',
    local: 'Local',
    remoteDown: 'Remote unreachable, using local',
    localOnlyMeeting: 'Local only for this meeting',
    newVersionTitle: 'A new version is available — click for release notes',
    newVersion: (v: string) => `New version ${v}`,
    styleAria: 'Board style',
    ink: 'Handwriting',
    pencil: 'Pencil · editable',
    log: 'Transcript',
    replay: 'Replay',
    exitReplay: 'Exit replay',
    replayBlocked: "Can't replay while listening or while the model is drawing",
    replayHint: 'Replay: watch the board grow step by step',
    undo: 'Undo',
    undoTitle: 'Undo (Ctrl+Z)',
    redo: 'Redo',
    redoTitle: 'Redo (Ctrl+Shift+Z)',
    tidy: 'Tidy layout',
    tidyTitle: 'Tidy layout: lay out the whole board again',
    import: 'Import',
    importTitle: 'Import an image, PDF, .excalidraw or arXiv paper (or drop a file on the board)',
    chooseFile: 'Choose file…',
    chooseFileHint: 'Images and PDFs go under the board for annotating; .excalidraw becomes board content',
    arxiv: 'arXiv paper',
    arxivPlaceholder: 'arxiv.org/abs/… or 2401.12345',
    arxivWebHint: "The web version can't download it — use the desktop app, or download the PDF and drop it here",
    export: 'Export',
    exportMd: 'Key points, Mermaid flowchart and transcript',
    exportPng: 'PNG image',
    exportPngInk: 'Handwritten board, as on screen',
    exportPngPencil: 'Pencil board, including your edits',
    exportSvg: 'SVG vector',
    exportSvgHint: 'Stays sharp when zoomed; paste into docs or keep editing',
    exportPdf: 'Whole meeting PDF',
    exportPdfHint: 'One page per board, then summary and transcript; pages are images, text is not selectable',
    exportHtml: 'Replay page',
    exportHtmlHint: 'A single .html for people who missed the meeting — open it to watch the board grow',
    exportExcalidrawHint: '.excalidraw, keep editing on excalidraw.com',
    newMeeting: 'New meeting',
    newMeetingTitle: 'New meeting: this one is already saved; start again from a blank board',
    settings: 'Settings',

    // —— Selection toolbar ——
    renameEdgePlaceholder: 'Text on the line, leave empty to remove',
    renamePlaceholder: 'New name',
    rename: 'Rename',
    ok: 'OK',
    markUnitAria: 'Mark scope',
    units: { word: 'Word', sentence: 'Sentence', block: 'Block' },
    /** Keys are the mark names stored in the graph (they stay as they are); values are button labels */
    marks: { 划掉: 'Strike', 圈出: 'Circle', 下划线: 'Underline', 星标: 'Star' } as Record<Mark, string>,
    addTerm: 'Add to glossary',
    addTermTitle: "Add this name to the meeting's glossary: speech and model output get corrected to it",
    delete: 'Delete',
    pencilLoading: 'Opening the pencil board…',

    // —— Empty board ——
    kicker: 'Local-first · live meeting diagrams',
    headline1: 'Just talk.',
    headline2: 'The whiteboard draws itself.',
    lead: (desktop: boolean) =>
      `What you say becomes topics, points, flows and questions, written stroke by stroke on the board. ${desktop ? 'Click the mic below to start, or play a sample meeting first.' : 'Type a sentence below, or play a sample meeting first.'}`,
    startMic: 'Start listening',
    playDemo: 'Play sample meeting',
    waitModel: 'Waiting for the model…',
    demo: 'Sample',
    demoAria: 'Sample meeting',

    // —— Transcript ——
    count: (n: number) => plural(n, 'line'),
    stopSumming: 'Stop generating',
    summaryCloud: (model: string, host: string) => `Summarize with cloud model ${model}: the whole transcript is sent to ${host}`,
    summaryLocal: 'Summarize with the local model: conclusions, decisions, to-dos, open questions',
    stop: 'Stop',
    resummarize: 'Summarize again',
    summarize: 'Summary',
    collapseLog: 'Hide transcript',
    renameFrom: 'Rename',
    renameTo: 'to',
    renameSpeaker: 'Rename',
    renameSpeakerTitle: (name: string) => `Rename "${name}"`,
    cancel: 'Cancel',
    logEmpty: 'Nobody has spoken yet. Every sentence, and what it drew, shows up here.',
    gotoPage: 'Go to this board',
    pageN: (n: number) => `Board ${n}`,
    editBadge: 'Edit',
    drawing: 'Drawing…',
    lineFellBack: ' · remote failed, local model drew this one',
    noChange: '(no changes)',

    // —— Manual edits, as written into the transcript ——
    undoNote: 'Undid the last step',
    redoNote: 'Redid a step',
    movedNote: (what: string) => `Moved ${what}`,
    deletedNote: (what: string) => `Deleted ${what}`,
    markSpanNote: (removed: boolean, text: string, mark: string) => `${removed ? 'Removed' : 'Added'} ${mark} on "${text}"`,
    markNote: (removed: boolean, what: string, mark: string) => `${removed ? 'Removed' : 'Added'} ${mark} on ${what}`,
    renamedNote: (before: string, after: string) => `Renamed ${before} to "${after || '(no label)'}"`,

    // —— Toasts ——
    noSuchPage: "There's no board with that number",
    busyPage: 'The model is still drawing — switch boards when it is done',
    busyImport: 'The model is still drawing — import when it is done',
    replayNoPage: "Can't switch boards during replay — exit replay first",
    needVoiceModel: 'Download the speech recognition model first',
    summing: 'Summarizing…',
    summingPart: (done: number, total: number) => `Summarizing part ${done}/${total}…`,
    summingMerge: 'Putting the summary together…',
    summaryFellBack: 'Remote model failed; the summary used the local model',
    summaryFailed: (e: string) => `Summary failed: ${e}`,
    saved: (where: string) => `Saved: ${where}`,
    downloaded: (where: string) => `Downloaded: ${where}`,
    exportFailed: (e: string) => `Export failed: ${e}`,
    importFailed: (e: string) => `Import failed: ${e}`,
    openFailed: "Couldn't open the meeting: it no longer exists",
    termExists: (label: string) => `"${label}" is already in the glossary`,
    termAdded: (label: string) => `Added "${label}" to this meeting's glossary — add common mishearings in Settings`,
    copied: 'Error details copied',
    copyError: 'Copy error details',

    // —— Import ——
    reading: (name: string) => `Reading ${name}…`,
    readingPages: (name: string, done: number, total: number) => `Reading ${name}: page ${done}/${total}`,
    refsSkipped: (n: number) => `${plural(n, 'page')} of references skipped`,
    unread: (n: number) => `board limit reached, last ${plural(n, 'page')} not imported`,
    scanned: (n: number) => `${plural(n, 'page')} without a text layer, imported as images`,
    pdfNote: (name: string, total: number, boards: number, extra: string[]) =>
      `Imported ${name} (${plural(total, 'page')}, ${plural(boards, 'board')}${extra.length ? `; ${extra.join('; ')}` : ''})`,
    pdfReadyPaper: (name: string) => `${name} is ready: split by sections, figures and equations — try "here in figure 3" or "equation 5 is wrong"`,
    pdfReady: (name: string) => `${name} is ready: one board per page — try "circle this paragraph"`,
    arxivBad: "That's not an arXiv ID: paste an arxiv.org/abs/… link or an ID like 2401.12345",
    arxivWeb: "The web version can't download from arXiv (browsers block cross-site files): download the PDF and drop it here",
    arxivDownloading: (id: string) => `Downloading ${id} from arXiv…`,
    needVision: "The built-in model can't read images yet: download the vision add-on in Settings → Manage models",
    noLocal: 'No local model connected',
    looking: (name: string) => `Looking at ${name}…`,
    readingBlocks: (name: string, done: number, total: number) => `Reading ${name}: block ${done}/${total}`,
    imageNote: (name: string, read: number, capped: boolean) =>
      `Imported ${name} (${plural(read, 'text block')} read${capped ? ', too many blocks — only some were read' : ''})`,
    imageReady: (name: string) => `${name} is ready — try "circle this" during the meeting`,
    imageNoText: (name: string) => `No text found in ${name}; it can only be used as a background`,
    unsupported: 'You can import images, PDFs and .excalidraw files',
    excalidrawNote: (name: string, c: { nodes: number; edges: number; groups: number; notes: number }) =>
      `Imported ${name} (${[plural(c.nodes, 'box', 'boxes'), plural(c.edges, 'line'), c.groups && plural(c.groups, 'group'), c.notes && plural(c.notes, 'text')].filter(Boolean).join(', ')})`,
    importWhereAria: 'Where to import',
    importWhere: (name: string, boards: number) => `Where should ${name} (${plural(boards, 'board')}) go?`,
    importNewMeeting: 'New meeting',
    importHere: 'Add to this meeting',

    // —— Keyboard shortcuts ——
    keys: 'Keyboard shortcuts',
    close: 'Close',
    keysHint: 'Only in handwriting mode, and not while typing in a text field. During replay: Space plays or pauses, arrow keys step, Esc exits.',
  },
}

export type Messages = typeof en
