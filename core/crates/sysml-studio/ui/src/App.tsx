import { useCallback, useEffect, useRef, useState } from 'react';
import type { Camera, CanvasDocument, Display, ModeKey, Rect, ViewKey } from './types';
import { edit, fetchDocument, fetchFile, fetchFingerprint, fetchPlan, fetchSource, redoEdit, saveFile, undoEdit, type Plan } from './api';
import { judgePoll } from './poll';
import { Canvas, fitCamera, type Placement } from './Canvas';
import { Editor } from './Editor';
import { Inspector } from './Inspector';
import type { AlignMode } from './align';
import { Navigator, type OutlineEntry } from './Navigator';
import { CommandPalette, type Command } from './Palette';
import { Menu } from './Menu';
import { displayMenu, marksMenu } from './menus';
import { elementOnLine, flattenOutline } from './link';
import { BottomPanel, type BottomTab } from './BottomPanel';
import { MIN_READABLE_ZOOM } from './geometry';
import { lang, plural, setLang, t, type Lang } from './i18n';
import { alignRects } from './align';
import { diffDocuments, isClean, type Marks } from './marks';
import { buildQueue } from './queue';
import type { ViewEntry } from './types';

const VIEWS: ViewKey[] = ['gv', 'iv', 'tv', 'pv', 'stv', 'afv', 'sv'];
const VIEW_TITLE: Record<ViewKey, string> = {
  gv: 'general view: the structure diagram',
  stv: 'state transition view: states and the transitions between them',
  afv: 'action flow view: actions and the successions between them',
  sv: 'sequence view: parts as lifelines, messages in the order they are written',
  iv: 'interconnection view: parts and connectors',
  tv: 'traceability view: requirements and implementers, unmet marked red',
  pv: 'package view: the namespace tree, packages as containers',
};

const MODES: ModeKey[] = ['extended', 'compat'];
const MODE_TITLE: Record<ModeKey, string> = {
  extended: 'extended: satisfy edges and inherited ports',
  compat: 'compat: mirrors the reference implementation node by node',
};

const LANGS: Lang[] = ['en', 'zh'];

/** Which compartments the boxes draw. Persisted: a person who hid stereotypes wants them hidden. */
const DISPLAY_KEY = 'sysml-studio.display';
const DISPLAY_ALL: Display = { stereotype: true, members: true, ports: true };

/** Button label and tooltip per compartment, in display order. */
const DISPLAY_PARTS: Array<[keyof Display, string, string]> = [
  ['stereotype', '«»', 'Show or hide the stereotype line'],
  ['members', '≡', 'Show or hide the member compartment'],
  ['ports', '□', 'Show or hide the ports'],
];

function storedDisplay(): Display {
  try {
    const saved = JSON.parse(localStorage.getItem(DISPLAY_KEY) ?? 'null') as Partial<Display> | null;
    if (!saved) return DISPLAY_ALL;
    return {
      stereotype: saved.stereotype ?? true,
      members: saved.members ?? true,
      ports: saved.ports ?? true,
    };
  } catch {
    return DISPLAY_ALL;
  }
}

export default function App() {
  const [doc, setDoc] = useState<CanvasDocument | null>(null);
  const [view, setView] = useState<ViewKey>('gv');
  const [mode, setMode] = useState<ModeKey>('extended');
  const [selection, setSelection] = useState<string[]>([]);
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, zoom: 1 });
  const [overrides, setOverridesState] = useState<Record<string, Rect>>({});
  const [display, setDisplay] = useState<Display>(storedDisplay);
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);  const [busy, setBusy] = useState(false);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  const [views, setViews] = useState<ViewEntry[]>([]);
  /** The model-declared view being shown, or null for a built-in projection over the whole model. */
  const [modelView, setModelView] = useState<string | null>(null);
  const modelViewRef = useRef<string | null>(null);
  modelViewRef.current = modelView;
  /**
   * The editor's buffer.
   *
   * `text` is what the person has typed; `saved` is what the server last sent. They differ only
   * while there are unsaved changes, which is what `dirty` reports — comparing the two is cheaper
   * and more honest than a boolean somebody has to remember to clear.
   */
  const [editor, setEditor] = useState<{ path: string | null; text: string; saved: string }>({
    path: null,
    text: '',
    saved: '',
  });
  const [saving, setSaving] = useState(false);
  const dirty = editor.text !== editor.saved;
  /**
   * The annotation filter as typed.
   *
   * The ref is what the one-second poll reads: rebuilding `load` on every keystroke would restart
   * the poll and the in-flight abort for each character typed.
   */
  const [filter, setFilter] = useState('');
  const filterRef = useRef('');
  filterRef.current = filter;
  /** The picture as it was when this view was opened; marks are differences from it. */
  const [baseline, setBaseline] = useState<CanvasDocument | null>(null);
  const [showMarks, setShowMarks] = useState(true);
  const [tab, setTab] = useState<BottomTab>('problems');
  const [queueActive, setQueueActive] = useState<string | null>(null);
  // The reviewer's own name, remembered so approving an assumption is one keystroke the second
  // time. It is what the provenance gate records as `human:<name>`, so it is typed by a person.
  const [reviewer, setReviewer] = useState(() => {
    try {
      return localStorage.getItem('sysml-studio.reviewer') ?? '';
    } catch {
      return '';
    }
  });
  const [panelClosed, setPanelClosed] = useState(false);
  // The primitive text is English; this only picks which translation table `t` reads.
  const [language, setLanguage] = useState<Lang>(lang());

  const viewport = useRef<HTMLDivElement>(null);
  const docRef = useRef<CanvasDocument | null>(null);
  const fingerprint = useRef<string | null>(null);
  const pending = useRef<AbortController | null>(null);
  /** Whether a poll is waiting for its answer. */
  const polling = useRef(false);
  /** How many loads have started. A poll answered after a later load started is out of date. */
  const loads = useRef(0);
  const dragging = useRef(false);
  const toastTimer = useRef<number | undefined>(undefined);

  const toast = useCallback((text: string, bad = false) => {
    setMessage({ text, bad });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setMessage(null), bad ? 6000 : 2600);
  }, []);

  const copyCommand = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast(t('Copied'));
    } catch {
      // A browser that refuses clipboard access still leaves the text selectable on screen.
      toast(t('Copy failed'), true);
    }
  }, [toast]);

  const load = useCallback(async (options: {
    view?: string;
    mode?: string;
    keepSelection?: boolean;
    quiet?: boolean;
    /** Which model-declared view to draw; null means the whole model. */
    only?: string | null;
    /** The annotation filter to draw by; undefined keeps whatever the box currently holds. */
    filter?: string;
    /**
     * Move the change-marks baseline to the document being loaded.
     *
     * Only the first load of a view does this. A poll-driven reload must not, or the marks would
     * be erased by the very reload that discovered the change — every external edit would arrive
     * already marked as old.
     */
    rebaseline?: boolean;
  } = {}): Promise<CanvasDocument | null> => {
    // A second load while one is in flight would interleave two documents; the last one wins.
    pending.current?.abort();
    const controller = new AbortController();
    pending.current = controller;
    loads.current += 1;
    // A poll-driven reload must not flash the busy badge: it happens every second and nothing is
    // visibly loading, so the badge would blink forever.
    if (!options.quiet) setBusy(true);
    try {
      const next = await fetchDocument(
        options.view ?? view,
        options.mode ?? mode,
        options.only === undefined ? modelViewRef.current : options.only,
        options.filter === undefined ? filterRef.current : options.filter,
        controller.signal,
      );
      docRef.current = next;
      // Keep the poll's baseline in step with what is actually on screen: without this the next
      // poll compares against a stale value and reports a change that already happened.
      fingerprint.current = next.fingerprint;
      setDoc(next);
      // The ladder is refetched on a real load only. A poll-driven reload happens every second and
      // the stage cannot change without an edit, which is itself a real load.
      if (!options.quiet) void fetchPlan(lang()).then(setPlan).catch(() => undefined);
      if (options.rebaseline) setBaseline(next);
      setOverridesState({});
      setSelection((current) => (
        (options.keepSelection ?? true)
          ? current.filter((key) => next.nodes.some((n) => n.key === key))
          : []
      ));
      return next;
    } catch (error) {
      if ((error as Error).name !== 'AbortError') toast((error as Error).message, true);
      return null;
    } finally {
      if (pending.current === controller) pending.current = null;
      if (!options.quiet) setBusy(false);
    }
  }, [mode, toast, view]);

  /**
   * Load a buffer into the editor.
   *
   * The first file is opened automatically: an editor showing nothing on a project that has one
   * file is a step nobody asked for. A later call with the same path keeps what is typed, so a
   * click on the canvas does not discard unsaved work — the reveal is a scroll, not a reload.
   */
  const openFile = useCallback(async (path: string) => {
    if (editor.path === path && editor.text !== '') return;
    try {
      const file = await fetchFile(path);
      if (!file.ok) {
        toast(file.error ?? t('Could not read the file'), true);
        return;
      }
      setEditor({ path: file.path, text: file.text, saved: file.text });
    } catch (error) {
      toast(error instanceof Error ? error.message : String(error), true);
    }
  }, [editor.path, editor.text, toast]);

  /** Write the buffer back. The server parses it first and refuses one that does not build. */
  const saveEditor = useCallback(async () => {
    if (!editor.path || !dirty || saving) return;
    setSaving(true);
    try {
      const answer = await saveFile(editor.path, editor.text, fingerprint.current);
      if (!answer.ok) {
        toast(answer.error ?? t('Could not save the file'), true);
        return;
      }
      setEditor((current) => ({ ...current, saved: current.text }));
      // The model changed on disk, so the picture is now about an older model.
      await load({ keepSelection: true });
      toast(t('Saved'));
    } catch (error) {
      toast(error instanceof Error ? error.message : String(error), true);
    } finally {
      setSaving(false);
    }
  }, [dirty, editor.path, editor.text, load, saving, toast]);

  // The first file opens once the document names one.
  useEffect(() => {
    if (editor.path || !doc?.files.length) return;
    void openFile(doc.files[0]);
  }, [doc?.files, editor.path, openFile]);

  const fit = useCallback(() => {
    const box = viewport.current?.getBoundingClientRect();
    if (!box) return;
    const next = fitCamera(docRef.current, box);
    if (next) setCamera(next);
  }, []);

  useEffect(() => {
    void load({ keepSelection: false, rebaseline: true }).then(fit);
    // The first load is the app's bootstrap; later ones are triggered by the handlers below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-read when the source changes underneath us.
  //
  // This is the other half of "the source is the truth": an edit made in another editor has to
  // reach the picture without the person being told to press refresh, or they end up reviewing a
  // diagram that no longer matches the file. The poll asks the server for the digest of the files
  // and compares it with the digest of the document on screen: one short string, for which the
  // server parses nothing. It used to ask for the whole document every second. On a 500-line model
  // that took four seconds to build, so the answers fell further behind with every tick and each
  // edit waited behind the queue.
  //
  // Polling rather than a file watcher: a watcher is another dependency and another thing that
  // can miss an event (a network drive, an editor that writes via rename).
  useEffect(() => {
    const timer = window.setInterval(async () => {
      // Not while a drag is in flight: a reload mid-drag would rebuild the node the pointer is
      // holding and drop the gesture.
      if (dragging.current) return;
      // One poll at a time, and none while a load is in flight. The server answers one request at
      // a time, so a poll sent before the last one came back only queues behind it, and a load
      // brings its own digest.
      if (polling.current || pending.current) return;
      polling.current = true;
      const started = loads.current;
      try {
        const answered = await fetchFingerprint();
        const verdict = judgePoll({
          shown: fingerprint.current,
          answered,
          superseded: loads.current !== started,
        });
        if (verdict === 'ignore') return;
        fingerprint.current = answered;
        if (verdict === 'changed') {
          await load({ quiet: true });
          toast(t('The model changed on disk. The picture has been reloaded.'));
        }
      } catch {
        /* a poll that fails is not worth a message; the next one reports the real state */
      } finally {
        polling.current = false;
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [load, toast]);

  /**
   * Take back the last edit.
   *
   * The server owns the history, so this asks it to restore rather than reconstructing a buffer the
   * page never held: a rename can span several files, and the page only ever saw the result.
   */
  const undo = useCallback(async () => {
    try {
      const result = await undoEdit();
      if (!result.undone) {
        toast(t('Nothing to take back'));
        return;
      }
      await load();
      toast(t('The last edit was taken back'));
    } catch (error) {
      toast(error instanceof Error ? error.message : String(error), true);
    }
  }, [load, toast]);

  /** Put back the edit an undo took away. */
  const redo = useCallback(async () => {
    try {
      const result = await redoEdit();
      if (!result.redone) {
        toast(t('Nothing to put back'));
        return;
      }
      await load();
      toast(t('The edit was put back'));
    } catch (error) {
      toast(error instanceof Error ? error.message : String(error), true);
    }
  }, [load, toast]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // A field handles its own Escape (put the old text back). Clearing the selection as well
      // would close the panel the person is typing in, which reads as "my edit was rejected".
      const target = event.target as HTMLElement | null;
      const typing = target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
      if (event.key === 'Escape' && !typing) setSelection([]);
      // `Ctrl+Shift+P` is the key every editor uses for this, and it has to win over the browser's
      // own binding — hence `preventDefault` before the palette is even open.
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 'p') {
        event.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      // `Ctrl+Shift+M` shows the problems, the key VS Code uses for the same panel.
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 'm') {
        event.preventDefault();
        setTab('problems');
        setPanelClosed(false);
        return;
      }
      if (event.key === 'f' && !typing && !event.metaKey && !event.ctrlKey) fit();
      if ((event.metaKey || event.ctrlKey) && event.key === '0') {
        event.preventDefault();
        fit();
      }
      // Undo is the page's until a field has focus: inside a text field the browser's own undo
      // knows the keystrokes, and taking the key away would make typing feel broken.
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z' && !typing) {
        event.preventDefault();
        if (event.shiftKey) void redo();
        else void undo();
      }
      // Ctrl+Y is the other spelling Windows taught; both mean the same thing.
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'y' && !typing) {
        event.preventDefault();
        void redo();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [fit, undo, redo]);

  /** Replace the selection, or toggle one key into it (ctrl/cmd/shift-click). */
  const select = useCallback((keys: string[], additive = false) => {
    setSelection((current) => {
      if (!additive) return keys;
      const chosen = new Set(current);
      for (const key of keys) {
        if (chosen.has(key)) chosen.delete(key);
        else chosen.add(key);
      }
      return [...chosen];
    });
  }, []);

  /**
   * The editor position a canvas selection points at.
   *
   * The canvas knows a box by its qualified name and the editor by a byte offset, so the two are
   * joined by asking the server for the element's own source range. Asking is better than putting
   * a line number on every node: the document is rebuilt on every edit, and a line carried through
   * it would be one more thing to keep true.
   */
  const [reveal, setReveal] = useState<
    { line: number; nameStart: number; nameEnd: number } | null
  >(null);
  useEffect(() => {
    // One box selected is one place in the source. Several boxes have no single place, so the
    // editor stays where the person left it.
    if (selection.length !== 1) return;
    let live = true;
    fetchSource(selection[0])
      .then((source) => {
        if (!live || !source.ok) return;
        setReveal({ line: source.line, nameStart: source.nameStart, nameEnd: source.nameEnd });
      })
      .catch(() => undefined);
    return () => { live = false; };
  }, [selection]);

  /**
   * The box declared on the line the editor cursor is on.
   *
   * The outline already carries a key and a line for every element, so this walks it rather than
   * asking the server: a click in the editor has to move the picture without waiting for a round
   * trip. The innermost declaration at or above the line wins, which is what a person means by
   * "the thing I am looking at".
   */
  const onCursor = useCallback((line: number) => {
    const key = elementOnLine(flattenOutline(outline), line);
    // Only when it changes: setting the same selection again would re-run the reveal effect and
    // pull the editor back to the declaration while the cursor is on a line inside it.
    if (key && !(selection.length === 1 && selection[0] === key)) {
      setSelection([key]);
    }
  }, [outline, selection]);

  // The navigator lists the source tree, so it reloads when the document does — an edit that adds
  // a member has to show up in the tree, or the panel is describing a model that no longer exists.
  useEffect(() => {
    let live = true;
    fetch('/api/outline')
      .then((response) => (response.ok ? response.json() : []))
      .then((entries: OutlineEntry[]) => { if (live) setOutline(entries); })
      .catch(() => { /* the navigator is secondary: a failed fetch leaves the last tree */ });
    // The views the model declares, so the selector is built from the model rather than from a
    // list in this file. A failed fetch leaves the previous list, like the navigator.
    fetch('/api/views')
      .then((response) => (response.ok ? response.json() : []))
      .then((entries: ViewEntry[]) => { if (live) setViews(entries); })
      .catch(() => { /* the built-in projections still work */ });
    return () => { live = false; };
  }, [doc?.fingerprint]);

  /** Put a box in the middle of the viewport at the current zoom. */
  const centerOn = useCallback((target: CanvasDocument, key: string) => {
    const node = target.nodes.find((n) => n.key === key);
    const box = viewport.current?.getBoundingClientRect();
    if (!node || !box) return;
    const zoom = Math.max(camera.zoom, MIN_READABLE_ZOOM);
    setCamera({
      zoom,
      x: box.width / 2 - (node.x + node.w / 2) * zoom,
      y: box.height / 2 - (node.y + node.h / 2) * zoom,
    });
  }, [camera.zoom]);

  /**
   * Show an element from the navigator or a problem list.
   *
   * No single view draws everything: `gv` drops packages, `pv` drops features. If the key is not
   * in the current picture the view is switched to one that has it, rather than telling the person
   * to go find it themselves. The probe fetch is only paid on a miss.
   */
  /**
   * Select an element if the picture draws it, and say whether it did.
   *
   * A diagnostic's `at` is a qualified name, and a syntax error's is literally `syntax error` —
   * the parser has no element to name. The caller falls back to the line when this answers false,
   * so a click on the most common kind of problem still goes somewhere.
   */
  const locateIfFound = useCallback((key: string): boolean => {
    const current = docRef.current;
    if (!current?.nodes.some((n) => n.key === key)) return false;
    setSelection([key]);
    centerOn(current, key);
    return true;
  }, [centerOn]);

  /** Put the editor cursor on a line, for a diagnostic that names no element. */
  const locateLine = useCallback((line: number) => {
    // `nameStart`/`nameEnd` of 0 mean "scroll to the line, select nothing": the editor treats a
    // zero-length range at the top of the file as "no name to pick out".
    setReveal({ line, nameStart: 0, nameEnd: 0 });
  }, []);

  const locate = useCallback(async (key: string) => {
    const current = docRef.current;
    if (current?.nodes.some((n) => n.key === key)) {
      setSelection([key]);
      centerOn(current, key);
      return;
    }
    for (const candidate of ['pv', 'gv'] as ViewKey[]) {
      if (candidate === view) continue;
      let probe: CanvasDocument;
      try {
        probe = await fetchDocument(candidate, mode, null);
      } catch {
        continue;
      }
      if (!probe.nodes.some((n) => n.key === key)) continue;
      setView(candidate);
      const next = await load({ view: candidate, keepSelection: false });
      if (next) {
        setSelection([key]);
        centerOn(next, key);
      }
      return;
    }
    toast(t('No view draws this element.'), true);
  }, [centerOn, load, mode, toast, view]);

  /**
   * Every edit posts, then reloads the document from the server.
   *
   * The reload is deliberate: the server reparsed the source, so what comes back is what the file
   * now says. A canvas that patched its own state after an edit would drift from the file the
   * moment an edit was refused, and it would show a value the model does not have.
   */
  const submit = useCallback(async (path: string, body: Record<string, unknown>) => {
    const result = await edit(path, { ...body, fingerprint: docRef.current?.fingerprint });
    if (result.ok === false) return result;
    await load({ quiet: true });
    return result;
  }, [load]);

  /**
   * Save a set of rectangles.
   *
   * One route for one box and for eight: an alignment is one arrangement change, and posting it as
   * eight placements would write the sidecar eight times.
   */
  const place = useCallback(async (placements: Placement[]) => {
    try {
      await edit('/api/placements', { placements });
      // A moved box changes only the arrangement, so the whole document does not need rebuilding;
      // the edges stay where they were, which is what "I moved the box, not the lines" means.
      const moved = new Map(placements.map((p) => [p.key, p]));
      setOverridesState((current) => {
        const next = { ...current };
        for (const key of moved.keys()) delete next[key];
        return next;
      });
      setDoc((current) => current && {
        ...current,
        nodes: current.nodes.map((n) => {
          const rect = moved.get(n.key);
          return rect ? { ...n, x: rect.x, y: rect.y, w: rect.w, h: rect.h, placed: true } : n;
        }),
      });
    } catch (error) {
      toast((error as Error).message, true);
    }
  }, [toast]);

  /**
   * Move the selected boxes into a line, or space them evenly.
   *
   * The picture is a projection, so this is a layout change and nothing else: it posts
   * placements, it does not touch the model text. A box that was never moved has no placement yet,
   * and gets one here — otherwise aligning three boxes would align two of them.
   */
  const align = useCallback(async (how: AlignMode) => {
    const current = docRef.current;
    if (!current || selection.length < 2) return;
    const rects = selection
      .map((key) => {
        const rect = overrides[key] ?? current.nodes.find((n) => n.key === key);
        return rect ? { key, x: rect.x, y: rect.y, w: rect.w, h: rect.h } : null;
      })
      .filter((p): p is Placement => p !== null);
    if (rects.length < 2) return;
    await place(alignRects(rects, how));
  }, [overrides, place, selection]);

  // Switching view or mode replaces the picture, so there is nothing to compare against and the
  // baseline moves with it. Marks are only meaningful within one projection.
  const changeView = useCallback((next: ViewKey, only: string | null = null) => {
    setView(next);
    setModelView(only);
    setSelection([]);
    void load({ view: next, only, keepSelection: false, rebaseline: true }).then(fit);
  }, [fit, load]);

  const changeMode = useCallback((next: ModeKey) => {
    setMode(next);
    setSelection([]);
    void load({ mode: next, keepSelection: false, rebaseline: true }).then(fit);
  }, [fit, load]);

  const toggleDisplay = useCallback((part: keyof Display) => {
    setDisplay((current) => {
      const next = { ...current, [part]: !current[part] };
      try {
        localStorage.setItem(DISPLAY_KEY, JSON.stringify(next));
      } catch {
        /* the choice lasts for this session only */
      }
      return next;
    });
  }, []);

  /** Everything that differs from the baseline, or null when marks are off or nothing changed. */
  const marks: Marks | null = (() => {
    if (!showMarks || !baseline || !doc || baseline.view !== doc.view) return null;
    const computed = diffDocuments(baseline, doc);
    return isClean(computed) ? null : computed;
  })();

  const queue = doc ? buildQueue(doc) : [];

  const rememberReviewer = useCallback((name: string) => {
    setReviewer(name);
    try {
      localStorage.setItem('sysml-studio.reviewer', name);
    } catch {
      /* the name lasts for this session only */
    }
  }, []);

  /**
   * Re-judge one requirement.
   *
   * The judgement is written against the model as it stands, which is the only way to clear a
   * stale warning: re-recording the old verdict would be a lie with a newer timestamp.
   */
  const judge = useCallback(async (requirement: string) => {
    try {
      const result = await edit('/api/judge', { requirement });
      if (result.ok === false) { toast(result.error ?? t('Re-judging failed'), true); return; }
      await load({ quiet: true });
      setQueueActive(null);
    } catch (error) {
      toast((error as Error).message, true);
    }
  }, [load, toast]);

  const clearMarks = useCallback(() => {
    if (docRef.current) setBaseline(docRef.current);
  }, []);

  const changeLanguage = useCallback((next: Lang) => {
    setLang(next);
    setLanguage(next);
    // The ladder's text comes from the server in the chosen language, so it is fetched again.
    void fetchPlan(next).then(setPlan).catch(() => undefined);
  }, []);

  const reset = useCallback(async () => {
    const current = docRef.current;
    if (!current) return;
    try {
      for (const node of current.nodes) {
        if (node.placed) await edit('/api/reset-placement', { key: node.key });
      }
      await load({ quiet: true });
      fit();
    } catch (error) {
      toast((error as Error).message, true);
    }
  }, [fit, load, toast]);

  /**
   * Every action, in one list.
   *
   * The palette and the toolbar are two ways to reach the same functions; keeping one list means a
   * control added to one is not silently missing from the other. Titles are i18n keys, so the
   * palette reads in the interface language.
   */
  const commands: Command[] = [
    { id: 'Fit the picture', keys: 'f', run: fit },
    { id: 'Clear every manual position', run: () => void reset() },
    { id: 'Take back the last edit', keys: 'Ctrl+Z', run: () => void undo() },
    { id: 'Put the edit back', keys: 'Ctrl+Shift+Z', run: () => void redo() },
    {
      id: 'Save the file',
      keys: 'Ctrl+S',
      run: () => void saveEditor(),
      disabled: !dirty,
      reason: t('Nothing to save'),
    },
    ...VIEWS.map((key) => ({
      id: VIEW_TITLE[key],
      run: () => changeView(key),
    })),
    {
      id: 'Switch the model between compat and extended',
      run: () => changeMode(mode === 'extended' ? 'compat' : 'extended'),
    },
    {
      id: 'Show the problems',
      keys: 'Ctrl+Shift+M',
      run: () => {
        setTab('problems');
        setPanelClosed(false);
      },
    },
  ];
  return (
    <>
      <header className="bar">
        <div className="ident">
          <span className="mark">SysML</span>
          <span className="model">{doc?.title ?? t('Not loaded')}</span>
        </div>
        <div className="controls">
          <div className="seg" role="group" aria-label="view">
            {VIEWS.map((key) => (
              <button key={key} title={t(VIEW_TITLE[key])} className={key === view ? 'on' : ''} onClick={() => changeView(key)}>
                {key}
              </button>
            ))}
          </div>
          <div className="seg" role="group" aria-label="mode">
            {MODES.map((key) => (
              <button key={key} title={t(MODE_TITLE[key])} className={key === mode ? 'on' : ''} onClick={() => changeMode(key)}>
                {key}
              </button>
            ))}
          </div>
          {views.length > 0 && (
            <div className="seg" role="group" aria-label={t('Views the model declares')}>
              <button
                className={modelView === null ? 'on' : ''}
                title={t('Draw the whole model')}
                onClick={() => changeView(view, null)}
              >
                {t('whole model')}
              </button>
              {views.map((entry) => (
                <button
                  key={entry.key}
                  className={modelView === entry.key ? 'on' : ''}
                  title={
                    entry.projection
                      ? `${entry.key} : ${entry.target} → ${entry.projection}`
                      : `${entry.key} : ${entry.target} — ${t('drawn with the current projection')}`
                  }
                  // A view type with no projection of its own keeps the one on screen. The view's
                  // own statement is its `expose`, and that is what opens; refusing the tab made a
                  // model's own `view def` unopenable.
                  onClick={() =>
                    changeView((entry.projection || view) as ViewKey, entry.key)
                  }
                >
                  {entry.name}
                </button>
              ))}
            </div>
          )}
          <button className="plain" title={t('Command palette (Ctrl+Shift+P)')} onClick={() => setPaletteOpen(true)}>⌘</button>
          <button className="plain" title={t('Zoom to fit')} onClick={fit}>fit</button>
          <button className="plain" title={t('Take back the last edit (Ctrl+Z)')} onClick={() => void undo()}>undo</button>
          <button className="plain" title={t('Put back the edit an undo took away (Ctrl+Shift+Z)')} onClick={() => void redo()}>redo</button>
          {/* The filter is applied on Enter rather than on every keystroke: a half-typed `@Saf` is
              not a filter, and re-projecting the model per character would make typing stutter. */}
          <input
            className="filter"
            value={filter}
            placeholder={t('@Safety{level >= 2}')}
            title={t('Show only what an annotation says. Enter applies, Escape clears.')}
            onChange={(event) => setFilter(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                void load({ filter });
              }
              if (event.key === 'Escape') {
                event.preventDefault();
                setFilter('');
                void load({ filter: '' });
              }
            }}
          />
          <Menu
            label="Display"
            t={t}
            items={displayMenu(
              DISPLAY_PARTS.map(([part, , title]) => ({ part, title })),
              display,
              LANGS,
              language,
              (part) => toggleDisplay(part as keyof Display),
              changeLanguage,
            )}
          />
          <Menu
            label="Marks"
            t={t}
            items={marksMenu(showMarks, () => setShowMarks(!showMarks), clearMarks)}
          />
        </div>
      </header>

      {/* The ladder, where the picture is: the canvas is a projection of the model, and which
          stage the model is in is a fact about the model, not about the picture. Same source as
          `sysml next`, so the two cannot disagree. */}
      {plan && (
        <div className="stage" title={plan.waiting.join('\n') || t('nothing outstanding')}>
          <span className="badge">{plan.number <= 6 ? `${plan.number}/6` : '✓'}</span>
          <span className="name">{t(plan.stage)}</span>
          <span className="waiting">{plan.waiting[0] ?? t('nothing outstanding')}</span>
          {/* The names in that sentence, as things you can click. The canvas keys a box by the
              qualified name, which is what the plan carries, so a chip lands on the box. */}
          {plan.targets.map((key) => (
            <button
              key={key}
              className="target"
              title={`${t('Go to this element')}  ${key}`}
              onClick={() => void locate(key)}
            >
              {key.split('::').pop()}
            </button>
          ))}
          {plan.command && (
            <code
              className="cmd"
              title={t('Copy the command')}
              onClick={() => void copyCommand(plan.command as string)}
            >
              {plan.command}
            </code>
          )}
        </div>
      )}

      <div className="workspace">
        <Navigator entries={outline} selection={selection} onLocate={(key) => void locate(key)} />
        <Canvas
          doc={doc}
          view={view}
          camera={camera}
          setCamera={setCamera}
          selection={selection}
          select={select}
          display={display}
          marks={marks}
          overrides={overrides}
          setOverrides={(rects) => {
            setOverridesState((current) => ({ ...current, ...rects }));
          }}
          onPlaced={(placements) => void place(placements)}
          onDragChange={(active) => { dragging.current = active; }}
          viewportRef={viewport}
        />
        <Editor
          path={editor.path}
          value={editor.text}
          onChange={(text) => setEditor((current) => ({ ...current, text }))}
          onSave={() => void saveEditor()}
          dirty={dirty}
          saving={saving}
          reveal={reveal}
          onCursor={onCursor}
        />
        <Inspector
          doc={doc}
          plan={plan}
          selection={selection}
          select={select}
          onAlign={(how) => void align(how)}
          submit={submit}
          toast={toast}
        />
        <BottomPanel
          doc={doc}
          selected={selection.length === 1 ? selection[0] : null}
          tab={tab}
          setTab={setTab}
          collapsed={panelClosed}
          setCollapsed={setPanelClosed}
          onLocate={(key) => void locate(key)}
          onLocateIfFound={locateIfFound}
          onLocateLine={locateLine}
          queue={queue}
          queueActive={queueActive}
          setQueueActive={setQueueActive}
          onJudge={judge}
          submit={submit}
          toast={toast}
          reviewer={reviewer}
          setReviewer={rememberReviewer}
        />
      </div>

      <footer className="status">
        <span className="path">{doc?.files[0] ?? ''}</span>
        {doc && doc.files.length > 1 && (
          <span className="dim">{t(plural(doc.files.length - 1, '{n} more file', '{n} more files'), { n: doc.files.length - 1 })}</span>
        )}
        <span className="grow" />
        <span className="dim">{t(plural(doc?.summary.nodes ?? 0, '{n} element', '{n} elements'), { n: doc?.summary.nodes ?? 0 })}</span>
        <span className="dim">{t(plural(doc?.summary.edges ?? 0, '{n} relationship', '{n} relationships'), { n: doc?.summary.edges ?? 0 })}</span>
        <span className={doc && doc.summary.errors > 0 ? 'bad' : 'dim'}>
          {t(plural(doc?.summary.errors ?? 0, '{n} error', '{n} errors'), { n: doc?.summary.errors ?? 0 })}
        </span>
        <span className="dim">{`${Math.round(camera.zoom * 100)}%`}</span>
      </footer>

      <CommandPalette
        open={paletteOpen}
        commands={commands}
        onClose={() => setPaletteOpen(false)}
      />
      {message && <div className={`toast${message.bad ? ' bad' : ''}`}>{message.text}</div>}
      {busy && <div className="busy"><span /></div>}
    </>
  );
}
