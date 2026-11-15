import { useCallback, useEffect, useRef, useState } from 'react';
import {
  CrossCircledIcon,
  ExclamationTriangleIcon,
  LayersIcon,
  MagnifyingGlassIcon,
  MoonIcon,
  SunIcon,
} from '@radix-ui/react-icons';
import type { Camera, CanvasDocument, Display, Rect, ViewKey } from './types';
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
import { useTheme } from './theme';
import { exportSceneSvg } from './export';
import { isEmbedded, onHost, reportDiagramState, reportSelection, revealInHost } from './host';

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

/** Short names for the navigator; VIEW_TITLE stays the long description for tooltips. */
const VIEW_LABEL: Record<ViewKey, string> = {
  gv: 'General view',
  iv: 'Interconnection view',
  tv: 'Traceability view',
  pv: 'Package view',
  stv: 'State view',
  afv: 'Action flow view',
  sv: 'Sequence view',
};

const VIEW_CHOICES = VIEWS.map((key) => ({
  key,
  label: VIEW_LABEL[key],
  title: VIEW_TITLE[key],
}));
const DEPTH_VIEWS = new Set<ViewKey>(['gv', 'stv', 'afv']);

const LANGS: Lang[] = ['en', 'zh'];

/** Which compartments the boxes draw. Persisted: a person who hid stereotypes wants them hidden. */
const DISPLAY_KEY = 'sysml-studio.display';
const DISPLAY_ALL: Display = { stereotype: true, members: true, ports: true };

/** Label key and tooltip per compartment, in display order. */
const DISPLAY_PARTS: Array<[keyof Display, string, string]> = [
  ['stereotype', 'Stereotype', 'Show or hide the stereotype line'],
  ['members', 'Members', 'Show or hide the member compartment'],
  ['ports', 'Ports', 'Show or hide the ports'],
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

/** Panel sizes in pixels, as the dividers leave them. */
type Layout = { left: number; right: number; editor: number; bottom: number };

// Versioned: the divider map changed while the interface was being rebuilt,
// and an old saved width can leave a new panel unreadably small.
// Versioned again: the previous saved sizes came from a layout whose bottom
// strip could take half the window, and they are kept rather than migrated.
const LAYOUT_KEY = 'sysml-studio.layout.v3';
const LAYOUT_DEFAULT: Layout = { left: 260, right: 288, editor: 400, bottom: 220 };
const LAYOUT_BOUNDS: Record<keyof Layout, [number, number]> = {
  left: [220, 420],
  right: [240, 460],
  editor: [300, 640],
  bottom: [150, 340],
};

/**
 * Saved sizes, clamped.
 *
 * A layout saved on a wide window would hide a panel on a narrow one, so the
 * bounds are applied on read rather than only while dragging.
 */
function storedLayout(): Layout {
  const clamp = (key: keyof Layout, value: unknown): number => {
    const [min, max] = LAYOUT_BOUNDS[key];
    const number = typeof value === 'number' && Number.isFinite(value) ? value : LAYOUT_DEFAULT[key];
    return Math.min(max, Math.max(min, Math.round(number)));
  };
  try {
    const saved = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? 'null') as Partial<Layout> | null;
    if (!saved) return LAYOUT_DEFAULT;
    return {
      left: clamp('left', saved.left),
      right: clamp('right', saved.right),
      editor: clamp('editor', saved.editor),
      bottom: clamp('bottom', saved.bottom),
    };
  } catch {
    return LAYOUT_DEFAULT;
  }
}

export default function App() {
  /**
   * Whether this page is the whole studio or the picture alone.
   *
   * A panel in an editor already has an editor, a navigator, an outline and a Problems panel. The
   * picture is what it lacks, so the embedded page draws the picture and the inspector and leaves
   * the rest to the host — a second copy of each inside a webview is not "the canvas beside the
   * source", it is a whole application inside another one.
   */
  const embedded = isEmbedded();
  const [doc, setDoc] = useState<CanvasDocument | null>(null);
  const [view, setView] = useState<ViewKey>('gv');
  const [selection, setSelection] = useState<string[]>([]);
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, zoom: 1 });
  const [overrides, setOverridesState] = useState<Record<string, Rect>>({});
  const [display, setDisplay] = useState<Display>(storedDisplay);
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  const [views, setViews] = useState<ViewEntry[]>([]);
  /** The model-declared view being shown, or null for a built-in projection over the whole model. */
  const [modelView, setModelView] = useState<string | null>(null);
  const [scope, setScope] = useState<string | null>(null);
  const [depth, setDepth] = useState<number | null>(null);
  const [layoutStyle, setLayoutStyle] = useState<'nested' | 'tree'>('nested');
  const [diagramOptionsOpen, setDiagramOptionsOpen] = useState(false);
  const modelViewRef = useRef<string | null>(null);
  const scopeRef = useRef<string | null>(null);
  const depthRef = useRef<number | null>(null);
  const layoutRef = useRef<'nested' | 'tree'>('nested');
  modelViewRef.current = modelView;
  scopeRef.current = scope;
  depthRef.current = depth;
  layoutRef.current = layoutStyle;
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
   * The annotation filter, when something sets one.
   *
   * The input that typed it is gone with the toolbar, so this is a ref with no writer today. It
   * stays because `load` and the one-second poll both read it: the poll has to keep asking for the
   * same filtered picture, and a filter that a later feature sets must not need the poll rebuilt.
   */
  const filterRef = useRef('');
  // The model as it was before each edit, newest last.
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
  // The problems and the review queue are open by default: they are what a person
  // looks at the bottom of the window for, and a folded strip only hid them.
  const [panelClosed, setPanelClosed] = useState(false);
  // The stage bar keeps one readable line and opens the detail on demand: the
  // waiting sentence, the targets and the command together were a wall of text.
  const [stageOpen, setStageOpen] = useState(false);
  const [layout, setLayout] = useState<Layout>(storedLayout);
  // The navigator is a sidebar the activity bar can put away, the way an IDE does.
  const [sidebarOpen, setSidebarOpen] = useState(true);
  // The primitive text is English; this only picks which translation table `t` reads.
  const [language, setLanguage] = useState<Lang>(lang());
  const [themeName, setThemeName] = useTheme();

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
    scope?: string | null;
    depth?: number | null;
    layout?: 'nested' | 'tree';
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
        options.mode ?? 'extended',
        options.only === undefined ? modelViewRef.current : options.only,
        options.filter === undefined ? filterRef.current : options.filter,
        controller.signal,
        options.scope === undefined ? scopeRef.current : options.scope,
        options.depth === undefined ? depthRef.current : options.depth,
        options.layout === undefined ? layoutRef.current : options.layout,
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
  }, [toast, view]);

  /**
   * Load a buffer into the editor.
   *
   * The first file is opened automatically: an editor showing nothing on a project that has one
   * file is a step nobody asked for. A later call with the same path keeps what is typed, so a
   * click on the canvas does not discard unsaved work — the reveal is a scroll, not a reload.
   */
  const openFile = useCallback(async (path: string) => {
    if (editor.path === path && editor.text !== '') return;
    // Switching files with unsaved changes would throw them away. The editor is one buffer, so
    // there is nowhere to keep them; the person is told and the click is not acted on.
    if (editor.text !== editor.saved) {
      toast(t('Save the current file before opening another'), true);
      return;
    }
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
  }, [editor.path, editor.saved, editor.text, toast]);

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

  /**
   * Fit after the browser has laid the new document out.
   *
   * The first load resolves before the workspace has its final width; measuring
   * then can produce the 0.08 fallback and leave the whole diagram as a speck.
   * Two frames put the read after style and layout have both settled.
   */
  const fitSoon = useCallback(() => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(fit);
    });
  }, [fit]);

  /** Fit once, the first time the picture pane has a size worth fitting to.
   *
   * The first document resolves before the grid has settled, so every explicit `fitSoon` can land
   * on a pane that is still a sliver. Watching the pane catches that first real measurement, and
   * the latch means a later splitter drag still only moves the divider instead of resetting zoom.
   */
  useEffect(() => {
    const node = viewport.current;
    if (!node || typeof ResizeObserver === 'undefined') return;
    let done = false;
    const observer = new ResizeObserver((entries) => {
      if (done) return;
      const rect = entries[0]?.contentRect;
      if (!rect || rect.width < 240 || rect.height < 240) return;
      done = true;
      fit();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [fit]);

  /** Zoom about the middle of the viewport, so the picture does not jump. */
  const zoomBy = useCallback((factor: number) => {
    const box = viewport.current?.getBoundingClientRect();
    if (!box) return;
    setCamera((current) => {
      const zoom = Math.min(4, Math.max(0.25, current.zoom * factor));
      const ratio = zoom / current.zoom;
      const cx = box.width / 2;
      const cy = box.height / 2;
      return {
        zoom,
        x: cx - (cx - current.x) * ratio,
        y: cy - (cy - current.y) * ratio,
      };
    });
  }, []);

  const exportSvg = useCallback(() => {
    if (exportSceneSvg()) toast(t('Exported'));
    else toast(t('Nothing to export'), true);
  }, [toast]);

  /** The divider being dragged: which edge, where the pointer started, its size then. */
  const split = useRef<{ id: keyof Layout; x: number; y: number; base: number } | null>(null);

  const beginSplit = (id: keyof Layout) => (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    split.current = { id, x: event.clientX, y: event.clientY, base: layout[id] };
  };

  const moveSplit = (event: React.PointerEvent<HTMLDivElement>) => {
    const current = split.current;
    if (!current) return;
    const dx = event.clientX - current.x;
    const dy = event.clientY - current.y;
    // The left edge and the bottom edge grow with the pointer. The two right-hand
    // edges grow against it, because their panel is anchored to the far side.
    const raw =
      current.id === 'left' ? current.base + dx
        : current.id === 'right' ? current.base - dx
          : current.id === 'editor' ? current.base - dx
            : current.base - dy;
    const [min, max] = LAYOUT_BOUNDS[current.id];
    const value = Math.min(max, Math.max(min, Math.round(raw)));
    setLayout((was) => (was[current.id] === value ? was : { ...was, [current.id]: value }));
  };

  const endSplit = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!split.current) return;
    split.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  // A divider someone moved once should stay where they put it.
  useEffect(() => {
    try {
      localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
    } catch {
      /* the size lasts for this session only */
    }
  }, [layout]);

  useEffect(() => {
    void load({ keepSelection: false, rebaseline: true }).then(fitSoon);
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
        // Embedded, the editor belongs to the host: the page has no caret of its own to move, so
        // the place is sent on rather than set locally. The path travels too, because the project
        // may be several files and the host has to open the right one.
        if (embedded) {
          revealInHost({
            file: source.path,
            line: source.line,
            nameStart: source.nameStart,
            nameEnd: source.nameEnd,
          });
          return;
        }
        setReveal({ line: source.line, nameStart: source.nameStart, nameEnd: source.nameEnd });
      })
      .catch(() => undefined);
    return () => { live = false; };
  }, [embedded, selection]);

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

  /**
   * The host's cursor and its selection.
   *
   * Embedded, the editor lives in the host, so its caret arrives as a message rather than as a
   * callback. The outline is in this page already, so the line becomes a key without a round trip;
   * and a key the host asks for is selected without the reveal bouncing back, which would fight
   * the caret the person just moved.
   */
  useEffect(() => {
    if (!embedded) return;
    return onHost({
      select: (key) => setSelection((current) => (
        current.length === 1 && current[0] === key ? current : [key]
      )),
      cursor: (_file, line) => {
        const key = elementOnLine(flattenOutline(outline), line);
        if (key) setSelection((current) => (
          current.length === 1 && current[0] === key ? current : [key]
        ));
      },
    });
  }, [embedded, outline]);

  /** Tell the host what is selected, so its own outline and status can follow the picture. */
  useEffect(() => {
    if (!embedded) return;
    reportSelection({ keys: selection });
  }, [embedded, selection]);

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
        probe = await fetchDocument(candidate, 'extended', null);
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
  }, [centerOn, load, toast, view]);

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
   *
   * The document is then reloaded. A moved box changes the arrangement, and the router computes
   * every line against the arrangement — so drawing the old polylines against new boxes is a
   * picture of two different layouts. The reload is cheap: the kernel re-reads nothing (the files
   * on disk did not change), and only the geometry is recomputed from the placements it now holds.
   *
   * While the drag was in flight the canvas shifted the ends of each line with the box it touches,
   * which is enough to see what is happening; this is what makes the result correct.
   */
  const place = useCallback(async (placements: Placement[]) => {
    try {
      await edit('/api/placements', { placements });
      // Keep drag geometry until load replaces boxes and routes together.
      setOverridesState((current) => ({
        ...current,
        ...Object.fromEntries(placements.map(({ key, ...rect }) => [key, rect])),
      }));
      // Quiet: a drag is one visible change, not a load. The box is already where the pointer left
      // it, and flashing the busy badge would say the picture is being rebuilt when it reads as
      // settled.
      await load({ quiet: true, keepSelection: true });
    } catch (error) {
      toast((error as Error).message, true);
    }
  }, [load, toast]);

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
  const changeView = useCallback(async (
    next: ViewKey,
    only: string | null = null,
    options: { scope?: string | null; depth?: number | null; layout?: 'nested' | 'tree' } = {},
  ) => {
    const nextScope = options.scope ?? null;
    const nextDepth = options.depth ?? null;
    const nextLayout = options.layout ?? layoutRef.current;
    if (nextScope && (only || next === 'iv' || next === 'tv' || next === 'pv' || next === 'sv')) {
      toast(t('Element scope is available for the general, state and action views.'), true);
      return null;
    }
    const document = await load({
      view: next, only, scope: nextScope, depth: nextDepth, layout: nextLayout,
      keepSelection: false, rebaseline: true,
    });
    if (!document) return null;
    setView(next);
    setModelView(only);
    setScope(nextScope);
    setDepth(nextDepth);
    setLayoutStyle(nextLayout);
    reportDiagramState({ view: next, only, scope: nextScope, depth: nextDepth, layout: nextLayout });
    setSelection([]);
    fitSoon();
    return document;
  }, [fitSoon, load]);

  useEffect(() => {
    if (!embedded) return;
    return onHost({ diagram: ({ view: next, only, focus, scope, depth, layout }) => {
      void changeView(next, only, { scope, depth, layout }).then((document) => {
        if (document && focus && document.nodes.some((node) => node.key === focus)) {
          setSelection([focus]);
          centerOn(document, focus);
        }
      });
    } });
  }, [embedded, changeView, centerOn]);

  const changeLayout = useCallback((next: 'nested' | 'tree') => {
    if (modelViewRef.current || view !== 'gv') return;
    void changeView(view, null, { scope: scopeRef.current, depth: depthRef.current, layout: next });
  }, [changeView, view]);

  const changeDepth = useCallback((next: number | null) => {
    if (modelViewRef.current || !DEPTH_VIEWS.has(view)) return;
    void changeView(view, null, { scope: scopeRef.current, depth: next, layout: layoutRef.current });
  }, [changeView, view]);

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
      fitSoon();
    } catch (error) {
      toast((error as Error).message, true);
    }
  }, [fitSoon, load, toast]);

  /**
   * Every action, in one list.
   *
   * The palette and the toolbar are two ways to reach the same functions; keeping one list means a
   * control added to one is not silently missing from the other. Titles are i18n keys, so the
   * palette reads in the interface language.
   */
  const commands: Command[] = [
    { id: 'Fit the picture', keys: 'f', run: fit },
    ...VIEWS.map((key) => ({ id: VIEW_TITLE[key], run: () => changeView(key) })),
    ...(view === 'gv' && !modelView ? [
      { id: 'Use nested diagram layout', run: () => changeLayout('nested') },
      { id: 'Use tree diagram layout', run: () => changeLayout('tree') },
    ] : []),
    ...(DEPTH_VIEWS.has(view) && !modelView ? [
      { id: 'Show one descendant level', run: () => changeDepth(1) },
      { id: 'Show two descendant levels', run: () => changeDepth(2) },
      { id: 'Show three descendant levels', run: () => changeDepth(3) },
      { id: 'Show four descendant levels', run: () => changeDepth(4) },
      { id: 'Show all descendants', run: () => changeDepth(null) },
    ] : []),
    { id: 'Take back the last edit', keys: 'Ctrl+Z', run: () => void undo() },
    { id: 'Put the edit back', keys: 'Ctrl+Shift+Z', run: () => void redo() },
    {
      id: 'Save the file',
      keys: 'Ctrl+S',
      run: () => void saveEditor(),
      disabled: !dirty,
      reason: t('Nothing to save'),
    },
    {
      id: 'Show the problems',
      keys: 'Ctrl+Shift+M',
      run: () => {
        setTab('problems');
        setPanelClosed(false);
      },
    },
    { id: 'Export SVG', run: exportSvg },
    { id: 'Clear every manual position', run: () => void reset() },
  ];
  /**
   * The camera, in the corner a camera belongs in.
   *
   * It has one home in both modes. Embedded it used to sit in the toolbar, which put a five-button
   * bar across the top of the picture; the picture is the point of the panel, and the only control
   * that has to sit next to it is the one that changes what you are looking at.
   */
  const zoomControls = (
    <div className="zoom-pill">
      <button title={t('Zoom out')} onClick={() => zoomBy(1 / 1.25)}>-</button>
      <span>{`${Math.round(camera.zoom * 100)}%`}</span>
      <button title={t('Zoom in')} onClick={() => zoomBy(1.25)}>+</button>
      <button title={t('Zoom to fit')} onClick={fit}>{t('Fit')}</button>
    </div>
  );
  return (
    <>
      {!embedded && (
      <header className="bar">
        <div className="ident">
          <span className="mark" aria-hidden="true">S</span>
          <span className="product">SysML Studio</span>
          <span className="ident-divider" />
          <span className="model">{doc?.title ?? t('Not loaded')}</span>
          <span className={`save-state${dirty ? ' dirty' : ''}`}>
            {dirty ? t('Unsaved changes') : t('Saved')}
          </span>
          {/* The stage is part of what the model is, so it belongs with the model's
              name rather than on a band of its own above the picture. */}
          {plan && (
            <button
              className={`stage-pill${plan.waiting.length > 0 ? ' waiting' : ''}`}
              title={plan.waiting[0] ?? t('nothing outstanding')}
              aria-expanded={stageOpen}
              onClick={() => setStageOpen((open) => !open)}
            >
              <span className="stage-pill-badge">{plan.number <= 6 ? `${plan.number}/6` : '✓'}</span>
              <span>{t(plan.stage)}</span>
              {plan.waiting.length > 0 && <span className="stage-pill-count">{plan.waiting.length}</span>}
            </button>
          )}
        </div>
        <div className="controls">
          <button
            className="command-search"
            title={t('Command palette (Ctrl+Shift+P)')}
            onClick={() => setPaletteOpen(true)}
          >
            <MagnifyingGlassIcon />
            <span>{t('Search commands or elements')}</span>
            <kbd>Ctrl Shift P</kbd>
          </button>
          <Menu
            label="Settings"
            t={t}
            items={[
              ...displayMenu(
                DISPLAY_PARTS.map(([part, , title]) => ({ part, title })),
                { stereotype: display.stereotype, members: display.members, ports: display.ports },
                LANGS,
                language,
                (part) => toggleDisplay(part as keyof Display),
                (code) => changeLanguage(code as Lang),
              ),
              ...marksMenu(showMarks, () => setShowMarks(!showMarks), clearMarks)
                .map((item, index) => ({ ...item, startsGroup: index === 0 })),
            ]}
          />
        </div>
        {plan && stageOpen && (
          <div className="stage-pop" role="dialog" aria-label={t('Stage')}>
            <div className="stage-pop-head">
              <span className="stage-pop-title">
                {`${plan.number <= 6 ? `${plan.number}/6` : '✓'} ${t(plan.stage)}`}
              </span>
              <button className="stage-pop-close" onClick={() => setStageOpen(false)}>{t('Close')}</button>
            </div>
            {plan.waiting.length > 0
              ? plan.waiting.map((line, index) => (
                <p key={index} className="stage-detail-line">{line}</p>
              ))
              : <p className="stage-detail-line">{t('nothing outstanding')}</p>}
            {plan.targets.length > 0 && (
              <div className="stage-targets">
                {plan.targets.map((key) => (
                  <button
                    key={key}
                    className="target"
                    title={`${t('Go to this element')}  ${key}`}
                    onClick={() => { void locate(key); setStageOpen(false); }}
                  >
                    {key.split('::').pop()}
                  </button>
                ))}
              </div>
            )}
            {plan.command && (
              <button className="stage-command" onClick={() => void copyCommand(plan.command as string)}>
                {t('Copy the next command')}
              </button>
            )}
          </div>
        )}
      </header>
      )}

      <div
        className={`workspace${embedded ? ' embedded' : ''}`}
        style={{
          '--left-w': sidebarOpen ? `${layout.left}px` : '0px',
          '--right-w': `${layout.right}px`,
        } as React.CSSProperties}
      >
        {!embedded && (
        <nav className="activity-bar" aria-label={t('Navigation')}>
          <button
            className={`activity${sidebarOpen ? ' on' : ''}`}
            title={t('Navigation')}
            onClick={() => setSidebarOpen((open) => !open)}
          >
            <LayersIcon />
          </button>
          <button
            className="activity"
            title={t('Problems')}
            onClick={() => {
              setTab('problems');
              setPanelClosed(false);
            }}
          >
            <ExclamationTriangleIcon />
            {(doc?.issues.length ?? 0) > 0 && (
              <span className="activity-badge">{doc?.issues.length}</span>
            )}
          </button>
          <span className="activity-spacer" />
          <button
            className="activity"
            title={themeName === 'dark' ? t('Light') : t('Dark')}
            onClick={() => setThemeName(themeName === 'dark' ? 'light' : 'dark')}
          >
            {themeName === 'dark' ? <SunIcon /> : <MoonIcon />}
          </button>
        </nav>
        )}
        {!embedded && (
        <Navigator
          entries={outline}
          files={doc?.files ?? []}
          current={editor.path}
          selection={selection}
          onLocate={(key) => void locate(key)}
          onOpenFile={(path) => void openFile(path)}
          views={VIEW_CHOICES}
          view={view}
          onView={(key) => changeView(key as ViewKey)}
          declaredViews={views}
          modelView={modelView}
          onModelView={(key) => {
            if (key === null) {
              changeView(view, null);
              return;
            }
            const entry = views.find((candidate) => candidate.key === key);
            changeView((entry?.projection || view) as ViewKey, key);
          }}
        />
        )}
        {!embedded && (
        <div
          className="splitter vertical splitter-left"
          role="separator"
          aria-orientation="vertical"
          aria-label={t('Drag to resize')}
          onPointerDown={beginSplit('left')}
          onPointerMove={moveSplit}
          onPointerUp={endSplit}
        />
        )}
        <div
          className="canvas-column"
          style={{
            '--editor-w': `${layout.editor}px`,
            '--bottom-h': `${layout.bottom}px`,
          } as React.CSSProperties}
        >
          <div className="canvas-wrap">
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
            {zoomControls}
            <div className="diagram-options">
              {diagramOptionsOpen && (
                <div className="diagram-options-panel" role="group" aria-label={t('Diagram options')}>
                  <label>{t('Diagram')}
                    <select value={modelView ? `declared:${modelView}` : view} onChange={(event) => {
                      const value = event.target.value;
                      if (value.startsWith('declared:')) {
                        const entry = views.find((candidate) => candidate.key === value.slice(9));
                        if (entry) void changeView((entry.projection || 'gv') as ViewKey, entry.key);
                      } else {
                        const next = value as ViewKey;
                        void changeView(next, null, DEPTH_VIEWS.has(next)
                          ? { scope, depth, layout: layoutStyle } : {});
                      }
                    }}>
                      {VIEWS.map((key) => <option key={key} value={key}>{t(VIEW_LABEL[key])}</option>)}
                      {views.map((entry) => <option key={entry.key} value={`declared:${entry.key}`}>{entry.name} — {t('Model-declared view')}</option>)}
                    </select>
                  </label>
                  {view === 'gv' && !modelView && (
                    <label>{t('Layout')}
                      <select value={layoutStyle} onChange={(event) => changeLayout(event.target.value as 'nested' | 'tree')}>
                        <option value="nested">{t('Nested')}</option>
                        <option value="tree">{t('Tree')}</option>
                      </select>
                    </label>
                  )}
                  {DEPTH_VIEWS.has(view) && !modelView && (
                    <label>{t('Depth')}
                      <select value={depth ?? 'all'} onChange={(event) => changeDepth(event.target.value === 'all' ? null : Number(event.target.value))}>
                        {[1, 2, 3, 4].map((value) => <option key={value} value={value}>{value}</option>)}
                        <option value="all">{t('Unlimited')}</option>
                      </select>
                    </label>
                  )}
                  <button className="diagram-options-toggle" disabled={selection.length !== 1}
                    onClick={() => void changeView('gv', null, { scope: selection[0], depth: 3, layout: layoutStyle })}>
                    {t('Visualize selected element')}
                  </button>
                  {(scope || modelView) && <button className="diagram-options-toggle" onClick={() => void changeView(view)}>{t('Show whole model')}</button>}
                  <button className="diagram-options-toggle" onClick={exportSvg}>{t('Export SVG')}</button>
                  <button className="diagram-options-toggle" onClick={() => void reset()}>{t('Clear every manual position')}</button>
                  <p>{scope ? `${t('Showing element')}: ${scope}` : modelView ? `${t('Model-declared view')}: ${modelView}` : depth !== null ? `${t('Showing the whole model')} · ${t('Depth')} ${depth}` : t('Showing the whole model')}</p>
                  {depth !== null && <p>{t('Deeper elements are summarized in a collapsed-count row.')}</p>}
                  {!scope && !modelView && <p>{t('To visualize one element, place the cursor on its declaration and run Visualize Element from the Command Palette.')}</p>}
                </div>
              )}
              <button className="diagram-options-toggle" type="button" aria-expanded={diagramOptionsOpen}
                title={t('Diagram options')} onClick={() => setDiagramOptionsOpen((open) => !open)}>
                {t('Diagram options')}
              </button>
            </div>
          </div>
          {!embedded && (
          <>
          <div
            className="splitter vertical splitter-editor"
            role="separator"
            aria-orientation="vertical"
            aria-label={t('Drag to resize')}
            onPointerDown={beginSplit('editor')}
            onPointerMove={moveSplit}
            onPointerUp={endSplit}
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
          <div
            className="splitter horizontal splitter-bottom"
            role="separator"
            aria-orientation="horizontal"
            aria-label={t('Drag to resize')}
            onPointerDown={beginSplit('bottom')}
            onPointerMove={moveSplit}
            onPointerUp={endSplit}
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
          </>
          )}
        </div>
        <div
          className="splitter vertical splitter-right"
          role="separator"
          aria-orientation="vertical"
          aria-label={t('Drag to resize')}
          onPointerDown={beginSplit('right')}
          onPointerMove={moveSplit}
          onPointerUp={endSplit}
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
      </div>

      {!embedded && (
      <footer className="status">
        <span
          className={`status-item${doc && doc.summary.errors > 0 ? ' bad' : ''}`}
          title={t(plural(doc?.summary.errors ?? 0, '{n} error', '{n} errors'), { n: doc?.summary.errors ?? 0 })}
        >
          <CrossCircledIcon />
          {doc?.summary.errors ?? 0}
        </span>
        <span className="status-item path">{doc?.files[0] ?? ''}</span>
        {doc && doc.files.length > 1 && (
          <span className="status-item dim">
            {t(plural(doc.files.length - 1, '{n} more file', '{n} more files'), { n: doc.files.length - 1 })}
          </span>
        )}
        <span className="grow" />
        <span className="status-item dim">{t(plural(doc?.summary.nodes ?? 0, '{n} element', '{n} elements'), { n: doc?.summary.nodes ?? 0 })}</span>
        <span className="status-item dim">{t(plural(doc?.summary.edges ?? 0, '{n} relationship', '{n} relationships'), { n: doc?.summary.edges ?? 0 })}</span>
        <span className="status-item dim">UTF-8</span>
        <span className="status-item dim">SysML</span>
        <span className="status-item dim">{`${Math.round(camera.zoom * 100)}%`}</span>
      </footer>
      )}

      {!embedded && (
      <CommandPalette
        open={paletteOpen}
        commands={commands}
        onClose={() => setPaletteOpen(false)}
      />
      )}
      {message && <div className={`toast${message.bad ? ' bad' : ''}`}>{message.text}</div>}
      {busy && <div className="busy"><span /></div>}
    </>
  );
}
