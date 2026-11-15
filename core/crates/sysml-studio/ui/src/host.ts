/**
 * The page as a guest of an editor.
 *
 * The studio is two things at once: a whole workspace with its own editor, navigator and problem
 * list, and the picture itself. A browser tab wants the first. An editor panel wants the second,
 * because the editor already has a navigator, an outline and a Problems panel, and a second copy
 * of each inside a webview is not "the canvas beside the source" — it is a whole application
 * inside another one.
 *
 * So the page asks the embedding host which of the two it is, and the host answers by putting
 * `sysml.host` in the fragment of the URL it loads:
 *
 *   - absent                     → standalone, the whole studio, today's behaviour
 *   - `#sysml.host=vscode`       → canvas and inspector only
 *
 * Nothing about this is specific to VS Code except the name, and the name is the key the host
 * messages are tagged with. A second editor adds a value, not a fork.
 *
 * The two directions:
 *
 *   page → host   `{ channel: 'sysml', type: 'reveal', file, line, nameStart?, nameEnd? }`
 *                 `{ channel: 'sysml', type: 'selection', keys: [...], file?, line? }`
 *   host → page   `{ channel: 'sysml', type: 'select', key }`
 *                 `{ channel: 'sysml', type: 'cursor', file, line }`
 *                 `{ channel: 'sysml', type: 'theme', appearance: 'dark' | 'light' }`
 *
 * `reveal` is the one that matters: a click on a box has to move the editor's caret to the
 * declaration, and the embedded page has no editor of its own to move it in.
 */

import type { ViewKey } from './types';

export type HostKind = 'standalone' | 'vscode';

export interface DiagramRequest {
  view: ViewKey;
  only: string | null;
  focus?: string;
  scope?: string | null;
  depth?: number | null;
  layout?: 'nested' | 'tree';
}

export function diagramRequest(value: unknown): DiagramRequest | null {
  if (!value || typeof value !== 'object') return null;
  const data = value as Record<string, unknown>;
  if (typeof data.view !== 'string' || !['gv', 'iv', 'pv', 'tv', 'stv', 'afv', 'sv'].includes(data.view)) return null;
  if (data.only !== undefined && data.only !== null && typeof data.only !== 'string') return null;
  if (data.focus !== undefined && typeof data.focus !== 'string') return null;
  if (data.scope !== undefined && data.scope !== null && typeof data.scope !== 'string') return null;
  if (data.depth !== undefined && data.depth !== null && (!Number.isInteger(data.depth) || (data.depth as number) < 1 || (data.depth as number) > 64)) return null;
  if (data.layout !== undefined && data.layout !== 'nested' && data.layout !== 'tree') return null;
  return {
    view: data.view as ViewKey,
    only: (data.only as string | null | undefined) ?? null,
    ...(typeof data.focus === 'string' ? { focus: data.focus } : {}),
    ...(typeof data.scope === 'string' ? { scope: data.scope } : {}),
    ...(typeof data.depth === 'number' ? { depth: data.depth } : {}),
    ...(data.layout === 'nested' || data.layout === 'tree' ? { layout: data.layout } : {}),
  };
}

export interface Reveal {
  file: string;
  /** 1-based, as the kernel's outline and the studio's own editor report lines. */
  line: number;
  /** Byte range of the element's name in the file, when the host should select it. */
  nameStart?: number;
  nameEnd?: number;
}

export interface Selection {
  keys: string[];
  file?: string;
  line?: number;
}

type Listener = Partial<{
  reveal: (where: Reveal) => void;
  selection: (what: Selection) => void;
  select: (key: string) => void;
  cursor: (file: string, line: number) => void;
  theme: (appearance: 'dark' | 'light') => void;
  palette: (colors: Record<string, string>) => void;
  diagram: (request: DiagramRequest) => void;
}>;

const listeners: Listener = {};
let pendingDiagram: DiagramRequest | null = null;

let installed = false;

/**
 * Which host this page is a guest of.
 *
 * Read from the fragment rather than a query string: the server serves one `index.html` for every
 * request, and a fragment never reaches it, so the standalone page and the embedded one are the
 * same bytes.
 *
 * `hash` is an argument so the rule can be tested in node, where there is no `window`: a copy of
 * the parsing in a test would be a second rule that drifts from this one.
 */
export function hostKind(hash: string = window.location.hash): HostKind {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  return params.get('sysml.host') === 'vscode' ? 'vscode' : 'standalone';
}

export function isEmbedded(): boolean {
  return hostKind() !== 'standalone';
}

/**
 * The appearance the host asked for in the fragment, when it named one.
 *
 * The `theme` message says the same thing, but a message arrives after the document is parsed and
 * drawn, so a light window showed a dark canvas until it landed. A fragment is readable before the
 * first paint, which is what makes the first frame correct rather than corrected — the same reason
 * `sysml.host` is in the fragment and not in a query string.
 */
export function hostAppearance(hash: string = window.location.hash): 'dark' | 'light' | null {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  const value = params.get('sysml.appearance');
  return value === 'light' || value === 'dark' ? value : null;
}

/** Whether a host is attached. Errors are swallowed: a page outside an editor has none. */
function post(message: Record<string, unknown>): void {
  try {
    window.parent?.postMessage({ channel: 'sysml', ...message }, '*');
  } catch {
    /* a sandboxed frame with no parent to talk to simply gets no answer */
  }
}

export function reportDiagramState(request: DiagramRequest): void {
  if (!isEmbedded()) return;
  post({ type: 'diagramState', ...request });
}

/** Tell the host where to put its editor's caret. */
export function revealInHost(where: Reveal): void {
  if (!isEmbedded()) return;
  post({ type: 'reveal', ...where });
}

/** Tell the host what the picture has selected. */
export function reportSelection(what: Selection): void {
  if (!isEmbedded()) return;
  post({ type: 'selection', ...what });
}

/**
 * Listen for the host's messages.
 *
 * Installed once, from `main.tsx`. The handlers are kept in one object and replaced by the next
 * `onHost` call, because the app is one component and there is one set of them.
 */
export function installHostBridge(): void {
  if (installed) return;
  installed = true;
  window.addEventListener('message', (event: MessageEvent) => {
    if (!isEmbedded() || event.source !== window.parent) return;
    const data = event.data as
      | {
          channel?: string;
          type?: string;
          appearance?: string;
          key?: string;
          file?: string;
          line?: number;
          colors?: Record<string, string>;
        }
      | null;
    if (!data || data.channel !== 'sysml') return;
    switch (data.type) {
      case 'diagram': {
        const request = diagramRequest(data);
        if (request) {
          if (listeners.diagram) listeners.diagram(request);
          else pendingDiagram = request;
        }
        break;
      }
      case 'select':
        if (typeof data.key === 'string') listeners.select?.(data.key);
        break;
      case 'cursor':
        if (typeof data.file === 'string' && typeof data.line === 'number') {
          listeners.cursor?.(data.file, data.line);
        }
        break;
      case 'theme':
        if (data.appearance === 'dark' || data.appearance === 'light') {
          listeners.theme?.(data.appearance);
        }
        break;
      case 'palette':
        if (data.colors) listeners.palette?.(data.colors);
        break;
    }
  });
}

/** Replace the handlers. Passing `null` for one of them stops it from being called. */
export function onHost(handlers: Partial<Listener>): () => void {
  Object.assign(listeners, handlers);
  if (handlers.diagram && pendingDiagram) {
    const request = pendingDiagram;
    pendingDiagram = null;
    handlers.diagram(request);
  }
  return () => {
    for (const key of Object.keys(handlers) as Array<keyof Listener>) {
      delete listeners[key];
    }
  };
}

/**
 * The studio's token names, in the order a VS Code colour fills them.
 *
 * `[token, --vscode-… variable, fallback]`. The fallback is the VS Code colour's own default, so a
 * variable a theme does not set leaves the interface looking like VS Code rather than like the
 * studio's own palette — a missing key must not make one panel a shade the rest is not.
 *
 * The variables are read on the host side and sent here as literal values, because a webview's
 * `--vscode-*` variables stop at the webview's own document: an iframe inside it is a different
 * document with its own, empty, set.
 */
const MAPPING: Array<[string, string, string]> = [
  ['--ui-bg', '--vscode-editor-background', '#1f1f1f'],
  ['--ui-canvas', '--vscode-editor-background', '#1f1f1f'],
  ['--ui-topbar', '--vscode-titleBar-activeBackground', '#181818'],
  ['--ui-panel', '--vscode-sideBar-background', '#181818'],
  ['--ui-raised', '--vscode-editorWidget-background', '#252526'],
  ['--ui-raised2', '--vscode-toolbar-hoverBackground', '#2d2d30'],
  ['--ui-border', '--vscode-panel-border', '#2b2b2b'],
  ['--ui-border-soft', '--vscode-editorWidget-border', '#232323'],
  ['--ui-line-strong', '--vscode-editorGroup-border', '#3c3c3c'],
  ['--ui-chip', '--vscode-badge-background', '#2d2d30'],
  ['--ui-text', '--vscode-editor-foreground', '#cccccc'],
  ['--ui-text2', '--vscode-foreground', '#cccccc'],
  ['--ui-text3', '--vscode-descriptionForeground', '#9d9d9d'],
  ['--ui-text4', '--vscode-disabledForeground', '#868686'],
  ['--ui-text5', '--vscode-disabledForeground', '#6e6e6e'],
  ['--ui-accent', '--vscode-focusBorder', '#0078d4'],
  ['--ui-accent-text', '--vscode-textLink-foreground', '#4daaf5'],
  ['--ui-accent-soft', '--vscode-list-activeSelectionBackground', '#04395e'],
  ['--ui-accent-soft-text', '--vscode-list-activeSelectionForeground', '#ffffff'],
  ['--ui-node-fill', '--vscode-editorWidget-background', '#252526'],
  ['--ui-node-header', '--vscode-editorGroupHeader-tabsBackground', '#2d2d30'],
  ['--ui-grid', '--vscode-editorIndentGuide-background1', '#262626'],
  ['--ui-edge', '--vscode-editorLineNumber-foreground', '#6e6e6e'],
  ['--ui-teal', '--vscode-testing-iconPassed', '#4ec9b0'],
  ['--ui-amber', '--vscode-editorWarning-foreground', '#cca700'],
  ['--ui-red', '--vscode-editorError-foreground', '#f14c4c'],
  ['--ui-warn-plate', '--vscode-inputValidation-warningBackground', '#33301a'],
  // Controls. The forms and menus were drawn with the studio's own greys, which are close to
  // VS Code's but not the same: a field whose border is `--ui-border` reads as a slightly wrong
  // box next to a VS Code input, because VS Code's input border is a different value from its
  // panel border and often absent entirely. These are the variables its own controls use.
  ['--ui-input-bg', '--vscode-input-background', '#313131'],
  ['--ui-input-fg', '--vscode-input-foreground', '#cccccc'],
  ['--ui-input-border', '--vscode-input-border', '#3c3c3c'],
  ['--ui-placeholder', '--vscode-input-placeholderForeground', '#9d9d9d'],
  ['--ui-dropdown-bg', '--vscode-dropdown-background', '#313131'],
  ['--ui-dropdown-fg', '--vscode-dropdown-foreground', '#cccccc'],
  ['--ui-dropdown-border', '--vscode-dropdown-border', '#3c3c3c'],
  ['--ui-menu-bg', '--vscode-menu-background', '#252526'],
  ['--ui-menu-fg', '--vscode-menu-foreground', '#cccccc'],
  ['--ui-menu-border', '--vscode-menu-border', '#454545'],
  ['--ui-menu-select-bg', '--vscode-menu-selectionBackground', '#0078d4'],
  ['--ui-menu-select-fg', '--vscode-menu-selectionForeground', '#ffffff'],
  ['--ui-menu-separator', '--vscode-menu-separatorBackground', '#454545'],
  ['--ui-shadow', '--vscode-widget-shadow', '#000000'],
  ['--ui-button-bg', '--vscode-button-background', '#0078d4'],
  ['--ui-button-fg', '--vscode-button-foreground', '#ffffff'],
  ['--ui-button-hover', '--vscode-button-hoverBackground', '#026ec1'],
  ['--ui-hover-bg', '--vscode-list-hoverBackground', '#2a2d2e'],
  ['--ui-hover-fg', '--vscode-list-hoverForeground', '#cccccc'],
  ['--ui-inactive-select-bg', '--vscode-list-inactiveSelectionBackground', '#37373d'],
];

/**
 * Paint the interface from the host's palette.
 *
 * Only tokens the host actually sent are set, so a colour a theme leaves undefined keeps the
 * built-in value. The theme's own `data-theme` attribute is left alone: it decides Radix's
 * appearance, and these variables win over both of its palettes by being set inline.
 *
 * The two button colours are also copied into the studio's own names, because a stylesheet in this
 * document cannot read a `--vscode-*` variable — those live in the host's document, and a value
 * read here would be empty. The names are ours so the rules can use them.
 */
export function applyHostPalette(colors: Record<string, string>): void {
  const root = document.documentElement;
  for (const [token, variable, fallback] of MAPPING) {
    const value = colors[variable]?.trim() || fallback;
    root.style.setProperty(token, value);
  }
  for (const [token, variable, fallback] of [
    ['--ui-toolbar-active', '--vscode-toolbar-activeBackground', '#2d2d30'],
    ['--ui-button-secondary', '--vscode-button-secondaryBackground', '#3a3d41'],
    ['--ui-button-secondary-text', '--vscode-button-secondaryForeground', '#cccccc'],
  ] as const) {
    root.style.setProperty(token, colors[variable]?.trim() || fallback);
  }
  // The type. VS Code's default font size is a bare number (`13`), not a CSS length, so it is
  // turned into one here rather than in the stylesheet: a variable holding `13` in a `font:`
  // shorthand makes the whole declaration invalid, and the panel would fall back to the browser's
  // 16px serif.
  const family = colors['--vscode-font-family']?.trim();
  if (family) root.style.setProperty('--ui-font', family);
  const size = Number.parseFloat(colors['--vscode-font-size']?.trim() ?? '');
  if (Number.isFinite(size) && size > 0) {
    root.style.setProperty('--ui-font-size', `${size}px`);
  }
  const monofamily = colors['--vscode-editor-font-family']?.trim();
  if (monofamily) root.style.setProperty('--ui-mono', monofamily);
  const monosize = Number.parseFloat(colors['--vscode-editor-font-size']?.trim() ?? '');
  if (Number.isFinite(monosize) && monosize > 0) {
    root.style.setProperty('--ui-mono-size', `${monosize}px`);
  }
}
