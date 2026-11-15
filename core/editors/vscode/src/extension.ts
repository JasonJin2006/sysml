import * as cp from 'child_process';
import * as fs from 'fs';
import * as http from 'http';
import * as path from 'path';
import * as vscode from 'vscode';
import { openGrid, type GridKind } from './grid';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let clientReady: Promise<void> | undefined;
let output: vscode.OutputChannel | undefined;

async function reportFailure(message: string, error: unknown): Promise<void> {
  output?.appendLine(`${message}\n${error instanceof Error ? error.message : String(error)}`);
  const action = await vscode.window.showErrorMessage(`${message} See SysML Output for details.`, 'Show Output');
  if (action === 'Show Output') output?.show(true);
}

/**
 * A kernel executable, found in one of three places.
 *
 * The setting comes first because a person who has pointed the extension at a binary means it.
 * Then the copy a packaged VSIX carries in `bin/<platform>-<arch>/`, which is what makes
 * "install and try it" one step. Last, the build a clone of this repository produces beside the
 * extension — `core/target/release/` relative to `core/editors/vscode/`.
 */
function findBinary(
  context: vscode.ExtensionContext,
  setting: string,
  name: string,
): string | null {
  const configured = vscode.workspace.getConfiguration('sysml').get<string>(setting, '');
  if (configured) {
    return configured;
  }
  const exe = process.platform === 'win32' ? `${name}.exe` : name;
  const platform = `${process.platform}-${process.arch}`;
  const candidates = [
    path.join(context.extensionPath, 'bin', platform, exe),
    path.resolve(context.extensionPath, '..', '..', 'target', 'release', exe),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

/**
 * The one studio this window is running, and the panel that shows it.
 *
 * The studio is a server, not a library: an edit is verified by reparsing with the same loader the
 * command line uses, and that loader lives in the kernel process. So the panel cannot compute a
 * view itself — it shows what the kernel serves. Two servers over one model would be two opinions
 * about the same files, so there is at most one, and it is stopped when its panel closes.
 */
interface DiagramState {
  view: string;
  only: string | null;
  scope: string | null;
  depth: number | null;
  layout: 'nested' | 'tree';
}

interface StudioSession {
  readonly files: string[];
  readonly panel: vscode.WebviewPanel;
  readonly server: cp.ChildProcess;
  readonly url: string;
  diagram: DiagramState;
}

let session: StudioSession | undefined;

/** The model to open: the file being edited, or one the person picks. */
async function chooseModel(resource?: vscode.Uri): Promise<string[] | null> {
  if (resource?.scheme === 'file' && /\.(sysml|kerml)$/i.test(resource.fsPath)) return [resource.fsPath];
  const editor = vscode.window.activeTextEditor;
  if (editor?.document.languageId === 'sysml') {
    if (editor.document.uri.scheme === 'file') return [editor.document.uri.fsPath];
    const choice = await vscode.window.showWarningMessage(
      'Save this model to a .sysml or .kerml file before opening its diagram.',
      { modal: true }, 'Save and Open', 'Cancel',
    );
    if (choice !== 'Save and Open' || !await editor.document.save()) return null;
    const saved = vscode.window.activeTextEditor?.document;
    return saved?.uri.scheme === 'file' ? [saved.uri.fsPath] : null;
  }

  const found = await vscode.workspace.findFiles(
    '**/*.{sysml,kerml}',
    '**/{target,node_modules,.git}/**',
    200,
  );
  if (found.length === 0) {
    void vscode.window.showWarningMessage(
      'No SysML or KerML files found. Open a .sysml or .kerml file, then choose Open Diagram.',
    );
    return null;
  }
  if (found.length === 1) {
    return [found[0].fsPath];
  }

  const items = found.map((uri) => ({
    label: vscode.workspace.asRelativePath(uri),
    description: path.dirname(uri.fsPath),
    detail: uri.fsPath,
  }));
  const picked = await vscode.window.showQuickPick(items, {
    placeHolder: 'Choose a model file to open as a diagram',
    matchOnDescription: true,
  });
  return picked ? [picked.detail] : null;
}

/**
 * Start the kernel on an ephemeral port and wait for the URL it prints.
 *
 * Port 0 rather than a fixed one: two windows on two projects are two normal things to have open,
 * and a fixed port turns the second into a failure. The kernel prints the port it was given, so
 * there is nothing to guess.
 */
function startServer(
  binary: string,
  files: string[],
): Promise<{ server: cp.ChildProcess; url: string }> {
  const cwd = path.dirname(files[0]);
  const server = cp.spawn(binary, ['studio', ...files, '--port', '0'], {
    cwd,
    windowsHide: true,
  });

  output?.appendLine(`Starting diagram server: ${binary}\nModel: ${files.join(', ')}`);
  return new Promise((resolve, reject) => {
    let printed = '';
    let settled = false;
    const timer = setTimeout(() => {
      stopServer(server);
      settle(new Error('The diagram server did not become ready within 15 seconds.'));
    }, 15_000);

    const settle = (error: Error | null, url?: string): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) reject(error);
      else resolve({ server, url: url as string });
    };

    server.stdout?.on('data', (chunk: Buffer) => {
      const text = chunk.toString();
      output?.append(text);
      if (settled) return;
      printed = (printed + text).slice(-8192);
      const found = /http:\/\/127\.0\.0\.1:\d+/.exec(printed);
      if (found) settle(null, found[0]);
    });
    server.stderr?.on('data', (chunk: Buffer) => output?.append(chunk.toString()));
    server.on('error', (error) => {
      output?.appendLine(`Diagram server error: ${error.message}`);
      settle(error);
    });
    server.on('exit', (code, signal) => {
      output?.appendLine(`Diagram server exited (${signal ?? code}).`);
      settle(new Error(`The diagram server stopped before it was ready (${signal ?? code}).`));
    });
  });
}

/** Kill the kernel, and on Windows the conhost wrapper that `spawn` does not reap. */
function stopServer(server: cp.ChildProcess): void {
  if (server.exitCode !== null || server.killed) {
    return;
  }
  server.kill();
  if (process.platform === 'win32' && server.pid !== undefined) {
    // A console child of a GUI process can outlive the handle Node holds; `taskkill` is the only
    // call that reliably reaches it. A failure here means it was already gone.
    cp.spawn('taskkill', ['/pid', String(server.pid), '/T', '/F'], { windowsHide: true }).on(
      'error',
      () => undefined,
    );
  }
}

/**
 * The panel's own document: a frame, and a relay.
 *
 * The studio is served over loopback as a whole page, so the panel does not rebuild it — it frames
 * it. The fragment `#sysml.host=vscode` is what tells the page it is a guest: as a guest it draws
 * the picture and the inspector and leaves the editor, the navigator and the Problems panel to this
 * window, which already has them. Without the fragment the page has no way to know, since a
 * fragment never reaches the server and both modes are the same bytes.
 *
 * The frame cannot talk to the extension directly — it is a plain web page in an iframe, not a
 * webview — so this document relays. A click on a box becomes `reveal`, which opens the file at the
 * declaration; a caret moved here becomes `cursor`, which selects the box. The content security
 * policy is narrowed to the one origin the kernel answered on, which is also why the address is
 * resolved first: under a remote window the frame is served through a forwarded port, and that is
 * a different origin from `127.0.0.1`.
 */
/**
 * The window's light-or-dark, in the two words the page understands.
 *
 * One function because three places need the same answer: the fragment that decides the first
 * paint, the message that follows a theme change, and nothing else may disagree about it. A
 * high-contrast theme counts as dark; a high-contrast light one does not.
 */
function appearance(): 'dark' | 'light' {
  const kind = vscode.window.activeColorTheme.kind;
  const light =
    kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight;
  return light ? 'light' : 'dark';
}

function panelHtml(address: string, embedded: string): string {
  const origin = new URL(address).origin;
  const nonce = String(Date.now());
  const policy = [
    "default-src 'none'",
    `frame-src ${origin}`,
    `style-src 'nonce-${nonce}'`,
    `script-src 'nonce-${nonce}'`,
  ].join('; ');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${policy}">
<style nonce="${nonce}">
  html, body { margin: 0; padding: 0; height: 100%; overflow: hidden; background: var(--vscode-editor-background); }
  iframe { display: block; width: 100%; height: 100%; border: 0; }
</style>
</head>
<body>
<iframe id="canvas" src="${embedded}" title="SysML Studio"></iframe>
<script nonce="${nonce}">
  const vscode = acquireVsCodeApi();
  const frame = document.getElementById('canvas');

  // The frame loads asynchronously, so a message sent before it is ready would be dropped. It is
  // held until the frame says it has loaded, and sent again afterwards — a canvas opened after a
  // tab switch must still be told what is selected.
  let ready = false;
  let pending = [];

  window.addEventListener('message', (event) => {
    const data = event.data;
    if (!data || data.channel !== 'sysml') return;
    if (data.type === 'reveal') {
      vscode.postMessage({ type: 'reveal', file: data.file, line: data.line });
      return;
    }
    if (data.type === 'selection') {
      vscode.postMessage({ type: 'selection', keys: data.keys });
      return;
    }
    if (data.type === 'diagramState') {
      vscode.postMessage({ type: 'diagramState', view: data.view, only: data.only,
        scope: data.scope, depth: data.depth, layout: data.layout });
    }
  });

  frame.addEventListener('load', () => {
    ready = true;
    for (const message of pending) frame.contentWindow.postMessage(message, '*');
    pending = [];
    tell();
  });

  window.addEventListener('message', (event) => {
    const data = event.data;
    if (!data || data.channel !== 'vscode-sysml') return;
    const message = { channel: 'sysml', ...data.message };
    if (ready) frame.contentWindow.postMessage(message, '*');
    else pending.push(message);
  });

  // The palette is the window's, and a canvas in one scheme inside an editor in another reads as
  // a broken panel. Two messages, because they answer two questions: the body class says light or
  // dark, which is what Radix and Monaco switch on, and the computed variables carry the editor's
  // actual colours — a theme may be dark and still be Dracula, and a picture painted in Dark
  // Modern's greys inside a Dracula window is the mismatch this exists to remove.
  const vars = [
    '--vscode-editor-background',
    '--vscode-editor-foreground',
    '--vscode-foreground',
    '--vscode-descriptionForeground',
    '--vscode-disabledForeground',
    '--vscode-titleBar-activeBackground',
    '--vscode-sideBar-background',
    '--vscode-editorWidget-background',
    '--vscode-editorWidget-border',
    '--vscode-toolbar-hoverBackground',
    '--vscode-panel-border',
    '--vscode-editorGroup-border',
    '--vscode-editorGroupHeader-tabsBackground',
    '--vscode-editorIndentGuide-background1',
    '--vscode-editorLineNumber-foreground',
    '--vscode-badge-background',
    '--vscode-badge-foreground',
    '--vscode-focusBorder',
    '--vscode-textLink-foreground',
    '--vscode-list-activeSelectionBackground',
    '--vscode-list-activeSelectionForeground',
    '--vscode-list-hoverBackground',
    '--vscode-list-hoverForeground',
    '--vscode-list-inactiveSelectionBackground',
    '--vscode-editorError-foreground',
    '--vscode-editorWarning-foreground',
    '--vscode-testing-iconPassed',
    '--vscode-inputValidation-warningBackground',
    '--vscode-inputValidation-errorBackground',
    '--vscode-toolbar-activeBackground',
    '--vscode-button-secondaryBackground',
    '--vscode-button-secondaryForeground',
    '--vscode-button-background',
    '--vscode-button-foreground',
    '--vscode-button-hoverBackground',
    '--vscode-button-border',
    '--vscode-input-background',
    '--vscode-input-foreground',
    '--vscode-input-border',
    '--vscode-input-placeholderForeground',
    '--vscode-dropdown-background',
    '--vscode-dropdown-foreground',
    '--vscode-dropdown-border',
    '--vscode-menu-background',
    '--vscode-menu-foreground',
    '--vscode-menu-border',
    '--vscode-menu-selectionBackground',
    '--vscode-menu-selectionForeground',
    '--vscode-menu-separatorBackground',
    '--vscode-widget-shadow',
    '--vscode-inputOption-activeBorder',
    '--vscode-checkbox-background',
    '--vscode-checkbox-border',
    '--vscode-font-family',
    '--vscode-font-size',
    '--vscode-editor-font-family',
    '--vscode-editor-font-size',
  ];
  const tell = () => {
    const computed = getComputedStyle(document.body);
    const colors = {};
    for (const name of vars) colors[name] = computed.getPropertyValue(name).trim();
    const dark = document.body.classList.contains('vscode-dark')
      || document.body.classList.contains('vscode-high-contrast');
    for (const message of [
      { channel: 'sysml', type: 'theme', appearance: dark ? 'dark' : 'light' },
      { channel: 'sysml', type: 'palette', colors },
    ]) {
      if (ready) frame.contentWindow.postMessage(message, '*');
      else pending.push(message);
    }
  };
  const observer = new MutationObserver(tell);
  const observed = { attributes: true, attributeFilter: ['class', 'style', 'data-vscode-theme-id', 'data-vscode-theme-kind'] };
  observer.observe(document.body, observed);
  observer.observe(document.documentElement, observed);
  setTimeout(tell, 0);
</script>
</body>
</html>`;
}

async function saveModelsBeforeOpen(files: readonly string[]): Promise<boolean> {
  const dirty = vscode.workspace.textDocuments.filter((doc) =>
    doc.isDirty && doc.uri.scheme === 'file' && files.includes(doc.uri.fsPath),
  );
  if (dirty.length === 0) return true;
  const choice = await vscode.window.showWarningMessage(
    'Save changes before opening the diagram? Diagrams use the saved model files.',
    { modal: true }, 'Save and Open', 'Cancel',
  );
  if (choice !== 'Save and Open') return false;
  try {
    for (const document of dirty) {
      if (!await document.save()) return false;
    }
    return true;
  } catch (error) {
    await reportFailure('The model could not be saved. The current diagram is unchanged.', error);
    return false;
  }
}

async function openStudio(context: vscode.ExtensionContext, resource?: vscode.Uri): Promise<StudioSession | null> {
  const files = await chooseModel(resource);
  if (!files || !await saveModelsBeforeOpen(files)) return null;
  const binary = findBinary(context, 'binary.path', 'sysml');
  if (!binary) {
    await reportFailure('Diagram engine not found. Set sysml.binary.path to the sysml executable, or reinstall the extension.', 'No diagram executable was found.');
    return null;
  }
  const previous = session;
  if (previous && previous.files.length === files.length && previous.files[0] === files[0]) {
    previous.panel.reveal(undefined, false);
    return previous;
  }

  let started: { server: cp.ChildProcess; url: string };
  try {
    started = await startServer(binary, files);
  } catch (error) {
    await reportFailure('Could not open the diagram. The current diagram is unchanged.', error);
    return null;
  }

  let address: string;
  let panel: vscode.WebviewPanel;
  try {
    address = (await vscode.env.asExternalUri(vscode.Uri.parse(started.url))).toString(true);
    panel = vscode.window.createWebviewPanel(
      'sysml.studio', `Diagram — ${path.basename(files[0])}`, vscode.ViewColumn.Beside,
      { enableScripts: true, retainContextWhenHidden: true },
    );
    panel.webview.html = panelHtml(address, `${address}#sysml.host=vscode&sysml.appearance=${appearance()}`);
  } catch (error) {
    stopServer(started.server);
    await reportFailure('Could not create the diagram panel. The current diagram is unchanged.', error);
    return null;
  }
  const running: StudioSession = {
    files, panel, server: started.server, url: started.url,
    diagram: { view: 'gv', only: null, scope: null, depth: null, layout: 'nested' },
  };
  session = running;

  /**
   * What the canvas asks this window to do.
   *
   * A click on a box names a file and a line, and the caret goes there — in the editor the person
   * is already reading, rather than in a second editor inside the panel. A selection arrives so
   * the title can say what is picked out; that is the one thing the panel's title has room for.
   */
  const messages = panel.webview.onDidReceiveMessage((message: {
    type?: string;
    file?: string;
    line?: number;
    keys?: string[];
    view?: string;
    only?: string | null;
    scope?: string | null;
    depth?: number | null;
    layout?: 'nested' | 'tree';
  }) => {
    if (message.type === 'reveal' && message.file && typeof message.line === 'number') {
      void revealInSource(message.file, message.line);
      return;
    }
    if (message.type === 'diagramState'
      && typeof message.view === 'string'
      && (message.layout === 'nested' || message.layout === 'tree')
      && (message.scope === null || typeof message.scope === 'string')
      && (message.depth === null || (typeof message.depth === 'number' && Number.isInteger(message.depth) && message.depth > 0))) {
      running.diagram = { view: message.view, only: message.only ?? null,
        scope: message.scope ?? null, depth: message.depth ?? null, layout: message.layout };
      return;
    }
    if (message.type === 'selection' && Array.isArray(message.keys)) {
      const only = message.keys.length === 1 ? message.keys[0].split('::').pop() : null;
      panel.title = only ? `Diagram — ${only}` : `Diagram — ${path.basename(files[0])}`;
    }
  });
  context.subscriptions.push(messages);

  /** Follow the window's theme, so the canvas is not a dark rectangle in a light window. */
  const syncTheme = (): void => {
    void panel.webview.postMessage({
      channel: 'vscode-sysml',
      message: { type: 'theme', appearance: appearance() },
    });
  };
  // Once now, and on every change. Registering the listener alone left the canvas on the page's
  // own default until the person happened to switch themes, which is why an editor panel opened
  // dark in a light window and stayed that way.
  syncTheme();
  const themeChanged = vscode.window.onDidChangeActiveColorTheme(syncTheme);

  /**
   * Follow the caret, so the picture highlights what the person is reading.
   *
   * Only a caret in a SysML file can name a box, and only a jump from a click is ignored — the
   * reveal this extension just asked for would otherwise come straight back as a selection and
   * move the canvas the person was about to look at.
   */
  let revealing = false;
  const cursorChanged = vscode.window.onDidChangeTextEditorSelection((event) => {
    if (revealing) return;
    if (event.textEditor.document.languageId !== 'sysml') return;
    if (event.textEditor.document.uri.scheme !== 'file') return;
    void panel.webview.postMessage({
      channel: 'vscode-sysml',
      message: {
        type: 'cursor',
        file: event.textEditor.document.uri.fsPath,
        line: event.selections[0]?.active.line + 1,
      },
    });
  });

  panel.onDidDispose(() => {
    themeChanged.dispose();
    cursorChanged.dispose();
    messages.dispose();
    stopServer(started.server);
    if (session === running) {
      session = undefined;
    }
  });
  previous?.panel.dispose();
  return running;

  /** Put the caret on a line of a file, opening it when it is not the one in front. */
  async function revealInSource(file: string, line: number): Promise<void> {
    const position = new vscode.Position(Math.max(0, line - 1), 0);
    const shown = vscode.window.visibleTextEditors.find(
      (editor) => editor.document.uri.fsPath === file,
    );
    const editor = shown ?? await vscode.window.showTextDocument(
      await vscode.workspace.openTextDocument(vscode.Uri.file(file)),
      { viewColumn: vscode.ViewColumn.One, preserveFocus: true },
    );
    revealing = true;
    editor.selection = new vscode.Selection(position, position);
    editor.revealRange(
      new vscode.Range(position, position),
      vscode.TextEditorRevealType.InCenterIfOutsideViewport,
    );
    // The selection event fires synchronously, so the flag is cleared on the next turn rather
    // than inside the event handler.
    setTimeout(() => { revealing = false; }, 0);
  }
}

const DIAGRAMS = [
  ['gv', 'General view', 'Definitions, usages and relationships'],
  ['iv', 'Interconnection view', 'Parts, ports and connections'],
  ['pv', 'Package view', 'Nested packages and namespaces'],
  ['stv', 'State transition view', 'States and transitions'],
  ['afv', 'Action flow view', 'Actions and successions'],
  ['sv', 'Sequence view', 'Lifelines and messages'],
  ['tv', 'Requirement traceability', 'Requirements and their satisfiers'],
] as const;

interface ModelView { key: string; name: string; projection: string; target: string }
interface DiagramChoice extends vscode.QuickPickItem { view: string; only: string | null }
interface DepthChoice extends vscode.QuickPickItem { depth: number | null }

interface DiagramDefaults { depth: number; layout: 'nested' | 'tree' }

function diagramDefaults(): DiagramDefaults {
  const config = vscode.workspace.getConfiguration('sysml.diagram');
  const depth = config.get<number>('defaultDepth', 3);
  const layout = config.get<string>('defaultElementRendering', 'nested');
  return {
    depth: Number.isInteger(depth) && depth >= 1 && depth <= 64 ? depth : 3,
    layout: layout === 'tree' ? 'tree' : 'nested',
  };
}

function pathAtPosition(
  symbols: readonly vscode.DocumentSymbol[],
  line: number,
  character: number,
): string | null {
  const contains = (range: vscode.Range) => {
    const afterStart = line > range.start.line || (line === range.start.line && character >= range.start.character);
    const beforeEnd = line < range.end.line || (line === range.end.line && character <= range.end.character);
    return afterStart && beforeEnd;
  };
  const walk = (items: readonly vscode.DocumentSymbol[], parents: string[]): string[] | null => {
    for (const symbol of items) {
      if (!contains(symbol.range) || !symbol.name || symbol.name.startsWith('<')) continue;
      const next = [...parents, symbol.name];
      const nested = walk(symbol.children, next);
      if (nested) return nested;
      return next;
    }
    return null;
  };
  const parts = walk(symbols, []);
  return parts?.join('::') || null;
}

function postDiagram(running: StudioSession, diagram: DiagramState): void {
  running.diagram = diagram;
  void running.panel.webview.postMessage({ channel: 'vscode-sysml', message: { type: 'diagram', ...diagram } });
}



async function changeDiagramDepth(): Promise<void> {
  const running = session;
  if (!running || !running.diagram.scope) {
    void vscode.window.showInformationMessage('Open an element-scoped diagram first with Visualize Element.');
    return;
  }
  const choices: DepthChoice[] = [
    ...[1, 2, 3, 4].map(depth => ({
      label: `Depth ${depth}`, description: `Show descendants up to ${depth} levels`, depth,
    })),
    { label: 'All', description: 'Show all descendants', depth: null },
  ];
  const picked = await vscode.window.showQuickPick(choices, {
    title: 'Diagram Depth', placeHolder: `Current depth: ${running.diagram.depth ?? 'all'}`,
  });
  if (!picked || session !== running) return;
  postDiagram(running, { ...running.diagram, depth: picked.depth });
}

function modelViews(url: string): Promise<ModelView[]> {
  return new Promise((resolve, reject) => {
    const request = http.get(new URL('/api/views', url), { timeout: 10_000 }, (response) => {
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`Model views request failed (HTTP ${response.statusCode}).`));
        return;
      }
      response.setEncoding('utf8');
      let body = '';
      response.on('data', (chunk: string) => {
        body += chunk;
        if (body.length > 1_000_000) request.destroy(new Error('Model views response is too large.'));
      });
      response.on('error', reject);
      response.on('end', () => {
        try {
          const value: unknown = JSON.parse(body);
          if (!Array.isArray(value) || value.some((item) => !item || typeof item.key !== 'string'
            || typeof item.name !== 'string' || typeof item.projection !== 'string' || typeof item.target !== 'string')) {
            throw new Error('The diagram server returned an invalid model views list.');
          }
          resolve(value as ModelView[]);
        } catch (error) { reject(error); }
      });
    });
    request.on('timeout', () => request.destroy(new Error('Loading model views timed out after 10 seconds.')));
    request.on('error', reject);
  });
}

async function chooseDiagram(context: vscode.ExtensionContext): Promise<void> {
  const running = session ?? await openStudio(context);
  if (!running || !await saveModelsBeforeOpen(running.files)) return;
  let declared: ModelView[];
  try {
    declared = await modelViews(running.url);
  } catch (error) {
    await reportFailure('Could not load model views. The current diagram is unchanged.', error);
    return;
  }
  const items: DiagramChoice[] = DIAGRAMS.map(([view, label, detail]) => ({
    label, description: 'Whole model', detail, view, only: null,
  }));
  for (const entry of declared) {
    items.push({ label: entry.name, description: 'Model-declared view',
      detail: `${entry.key} — ${entry.target}`, view: entry.projection, only: entry.key });
  }
  const picked = await vscode.window.showQuickPick(items, {
    title: 'Choose Diagram',
    placeHolder: declared.length ? 'Choose a diagram type or a model-declared view' : 'Choose a diagram type. No views are declared in this model.',
    matchOnDescription: true, matchOnDetail: true,
  });
  if (!picked || session !== running) return;
  if (!DIAGRAMS.some(([view]) => view === picked.view)) {
    void vscode.window.showInformationMessage('This view uses a diagram type that is not supported yet. Choose one of the built-in diagram types.');
    return;
  }
  postDiagram(running, { view: picked.view, only: picked.only, scope: null, depth: null, layout: 'nested' });
}

async function visualizeElement(context: vscode.ExtensionContext, selected?: vscode.Range): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'sysml' || editor.document.uri.scheme !== 'file') {
    void vscode.window.showInformationMessage('Open a saved SysML file and place the cursor in an element declaration.');
    return;
  }
  if (!client) {
    await reportFailure('SysML language features are not ready. Wait for the language server to start, then retry.', 'Language client is unavailable.');
    return;
  }
  const position = selected?.start ?? editor.selection.active;
  try {
    await clientReady;
    const symbols = await client.sendRequest<vscode.DocumentSymbol[]>(
      'textDocument/documentSymbol', { textDocument: { uri: editor.document.uri.toString() } },
    );
    const scope = pathAtPosition(symbols ?? [], position.line, position.character);
    if (!scope) {
      void vscode.window.showInformationMessage('No named model element at the cursor. Place it in an element declaration and try Visualize Element again.');
      return;
    }
    const running = await openStudio(context, editor.document.uri);
    if (!running) return;
    const defaults = diagramDefaults();
    const layout = await vscode.window.showQuickPick([
      { label: 'Nested', description: 'Arrange features with their owners', value: 'nested' as const, picked: defaults.layout === 'nested' },
      { label: 'Tree', description: 'Arrange the decomposition as a tree', value: 'tree' as const, picked: defaults.layout === 'tree' },
    ], { title: 'Visualize Element', placeHolder: `Choose a layout for ${scope}; configured default: ${defaults.layout}` });
    if (!layout || session !== running) return;
    postDiagram(running, { view: 'gv', only: null, scope, depth: defaults.depth, layout: layout.value });
  } catch (error) {
    await reportFailure('Could not identify the element at the cursor. The current diagram is unchanged.', error);
  }
}

async function showGrid(context: vscode.ExtensionContext, kind: GridKind): Promise<void> {
  const running = await openStudio(context);
  if (running) await openGrid(context, running.url, kind);
}

export function activate(context: vscode.ExtensionContext): void {
  output = vscode.window.createOutputChannel('SysML');
  context.subscriptions.push(output);
  context.subscriptions.push(
    vscode.commands.registerCommand('sysml.openStudio', (resource?: vscode.Uri) => openStudio(context, resource)),
    vscode.commands.registerCommand('sysml.chooseDiagram', () => chooseDiagram(context)),
    vscode.commands.registerCommand('sysml.openTable', () => showGrid(context, 'elements')),
    vscode.commands.registerCommand('sysml.openMatrix', () => showGrid(context, 'matrix')),
    vscode.commands.registerCommand('sysml.visualizeElement', (range?: vscode.Range) => visualizeElement(context, range)),
    vscode.commands.registerCommand('sysml.changeDiagramDepth', () => changeDiagramDepth()),
  );

  const command = findBinary(context, 'server.path', 'sysml-lsp');
  if (!command) {
    void reportFailure('Language server not found. Set sysml.server.path to the sysml-lsp executable, or reinstall the extension.', 'No language server executable was found.');
    return;
  }

  const serverOptions: ServerOptions = {
    run: { command, transport: TransportKind.stdio },
    debug: { command, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    outputChannel: output,
    documentSelector: [{ scheme: 'file', language: 'sysml' }],
    synchronize: {
      // Only the open documents are sent. The server checks what it is given; watching the
      // workspace would make it re-check files nobody is looking at.
      fileEvents: [],
    },
  };

  client = new LanguageClient('sysml', 'SysML v2', serverOptions, clientOptions);
  clientReady = client.start();
  clientReady.catch((error: unknown) => {
    void reportFailure('SysML language features could not start. Check the configured language server path.', error);
  });
  context.subscriptions.push({ dispose: () => void client?.stop() });
}

export function deactivate(): Thenable<void> | undefined {
  if (session) {
    stopServer(session.server);
    session = undefined;
  }
  return client?.stop();
}
