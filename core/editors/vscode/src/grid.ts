import * as http from 'http';
import { randomBytes } from 'crypto';
import * as vscode from 'vscode';

export type GridKind = 'elements' | 'matrix';

export interface GridData {
  title: string;
  description: string;
  columns: string[];
  rows: string[][];
  rowKeys: string[];
  csv: string;
}

export function parseGridData(value: unknown): GridData {
  if (!value || typeof value !== 'object') throw new Error('The model service returned invalid table data.');
  const data = value as Partial<GridData>;
  const strings = (items: unknown): items is string[] => Array.isArray(items) && items.every((item) => typeof item === 'string');
  if (typeof data.title !== 'string' || typeof data.description !== 'string' || typeof data.csv !== 'string'
      || !strings(data.columns) || data.columns.length === 0 || !strings(data.rowKeys)
      || !Array.isArray(data.rows) || data.rows.length !== data.rowKeys.length
      || !data.rows.every((row) => strings(row) && row.length === data.columns!.length)) {
    throw new Error('The model service returned invalid table data.');
  }
  return data as GridData;
}

export function gridRowOrder(data: GridData, query: string, column = -1, descending = false): number[] {
  const search = query.trim().toLocaleLowerCase();
  const indices = data.rows.map((_, index) => index).filter((index) =>
    !search || data.rows[index].some((cell) => cell.toLocaleLowerCase().includes(search)));
  if (column >= 0 && column < data.columns.length) {
    indices.sort((a, b) => {
      const compared = data.rows[a][column].localeCompare(data.rows[b][column], undefined, { numeric: true });
      return (descending ? -compared : compared) || a - b;
    });
  }
  return indices;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
}

export function renderGridHtml(data: GridData, kind: GridKind, nonce = randomBytes(18).toString('hex')): string {
  const encoded = JSON.stringify(data).replace(/</g, '\\u003c').replace(/>/g, '\\u003e')
    .replace(/&/g, '\\u0026').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<title>${escapeHtml(data.title)}</title>
<style nonce="${nonce}">
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { margin: 0; padding: 16px; color: var(--vscode-editor-foreground); background: var(--vscode-editor-background); font-family: var(--vscode-font-family); font-size: var(--vscode-font-size, 13px); }
h1 { font-size: 18px; font-weight: 600; margin: 0 0 8px; }
p { line-height: 1.5; max-width: 90ch; color: var(--vscode-descriptionForeground); margin: 6px 0; }
.controls { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 16px 0 8px; }
input { min-width: 160px; flex: 1; max-width: 440px; border: 1px solid var(--vscode-input-border, transparent); background: var(--vscode-input-background); color: var(--vscode-input-foreground); padding: 5px 7px; font: inherit; }
button { border: 1px solid var(--vscode-button-border, transparent); padding: 5px 9px; background: var(--vscode-button-secondaryBackground); color: var(--vscode-button-secondaryForeground); font: inherit; cursor: pointer; }
button:hover { background: var(--vscode-button-secondaryHoverBackground, var(--vscode-toolbar-hoverBackground)); }
button:disabled { cursor: default; opacity: .6; }
input:focus-visible,button:focus-visible,a:focus-visible { outline: 1px solid var(--vscode-focusBorder); outline-offset: 2px; }
#count { color: var(--vscode-descriptionForeground); padding: 6px 0 10px; }
.scroll { overflow: auto; max-height: calc(100vh - 220px); border: 1px solid var(--vscode-panel-border); }
table { border-collapse: separate; border-spacing: 0; min-width: 100%; font-size: inherit; }
th,td { text-align: left; padding: 7px 10px; border-bottom: 1px solid var(--vscode-panel-border); vertical-align: top; max-width: 560px; overflow-wrap: anywhere; }
th { position: sticky; top: 0; background: var(--vscode-editor-background); z-index: 1; font-weight: 600; }
th button { padding: 0; border: 0; background: transparent; color: inherit; font-weight: inherit; text-align: left; }
tr:hover td { background: var(--vscode-list-hoverBackground); }
a { color: var(--vscode-textLink-foreground); text-underline-offset: 2px; }
#problem { border-left: 3px solid var(--vscode-editorError-foreground); padding: 8px; background: var(--vscode-inputValidation-errorBackground); margin: 10px 0; }
#empty { margin: 16px 0; }
.matrix th:not(:first-child),.matrix td:not(:first-child) { text-align: center; min-width: 76px; }
[hidden] { display: none !important; }
</style></head><body>
<h1>${escapeHtml(data.title)}</h1>
<p id="description">${escapeHtml(data.description)}</p>
<p>Read-only. Shows saved model files. Save source changes, then refresh this view.</p>
<div class="controls"><input id="search" type="search" aria-label="Search all cells" placeholder="Search all cells"><button id="refresh" type="button">Refresh</button><button id="export" type="button" title="Export all rows, including rows hidden by the search">Export all rows as CSV</button></div>
<div id="problem" role="alert" hidden></div><div id="count" role="status" aria-live="polite"></div>
<div class="scroll"><table class="${kind === 'matrix' ? 'matrix' : 'elements'}"><thead><tr id="columns"></tr></thead><tbody id="rows"></tbody></table></div>
<p id="empty" hidden></p>
<script id="grid-data" type="application/json" nonce="${nonce}">${encoded}</script>
<script nonce="${nonce}">
const vscode = acquireVsCodeApi();
let data = JSON.parse(document.getElementById('grid-data').textContent);
let sortColumn = -1;
let descending = false;
const orderRows = ${gridRowOrder.toString()};
const columns = document.getElementById('columns');
const rows = document.getElementById('rows');
const search = document.getElementById('search');
const refresh = document.getElementById('refresh');
const problem = document.getElementById('problem');
function render() {
  columns.replaceChildren();
  data.columns.forEach((label, index) => {
    const cell = document.createElement('th');
    cell.scope = 'col';
    cell.setAttribute('aria-sort', sortColumn === index ? (descending ? 'descending' : 'ascending') : 'none');
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label + (sortColumn === index ? (descending ? ' ↓' : ' ↑') : '');
    button.title = 'Sort by ' + label;
    button.addEventListener('click', () => {
      descending = sortColumn === index ? !descending : false;
      sortColumn = index;
      render();
    });
    cell.appendChild(button);
    columns.appendChild(cell);
  });
  rows.replaceChildren();
  const order = orderRows(data, search.value, sortColumn, descending);
  order.forEach((index) => {
    const row = document.createElement('tr');
    data.rows[index].forEach((value, column) => {
      const cell = document.createElement('td');
      if (column === 0 && data.rowKeys[index]) {
        const link = document.createElement('a');
        link.href = '#';
        link.textContent = value || data.rowKeys[index];
        link.title = 'Open declaration in source';
        link.addEventListener('click', (event) => { event.preventDefault(); vscode.postMessage({ type: 'source', index }); });
        cell.appendChild(link);
      } else {
        cell.textContent = value;
      }
      row.appendChild(cell);
    });
    rows.appendChild(row);
  });
  document.getElementById('count').textContent = order.length + ' of ' + data.rows.length + ' rows. CSV export includes all ' + data.rows.length + ' rows.';
  const empty = document.getElementById('empty');
  empty.hidden = order.length > 0;
  empty.textContent = data.rows.length === 0 ? 'No rows in this model projection.' : 'No rows match the search. Clear the search to show all rows.';
  document.getElementById('description').textContent = data.description;
}
search.addEventListener('input', render);
refresh.addEventListener('click', () => { refresh.disabled = true; vscode.postMessage({ type: 'refresh' }); });
document.getElementById('export').addEventListener('click', () => vscode.postMessage({ type: 'export' }));
window.addEventListener('message', (event) => {
  const message = event.data;
  if (!message || typeof message !== 'object') return;
  if (message.type === 'data') {
    data = message.data;
    refresh.disabled = false;
    problem.hidden = true;
    render();
  } else if (message.type === 'error') {
    refresh.disabled = false;
    problem.textContent = message.message;
    problem.hidden = false;
  }
});
render();
</script></body></html>`;
}

function readJson(address: URL): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const request = http.get(address, (response) => {
      let text = '';
      response.setEncoding('utf8');
      response.on('data', (chunk: string) => { text += chunk; });
      response.on('error', reject);
      response.on('end', () => {
        if (response.statusCode !== 200) {
          try { reject(new Error(JSON.parse(text).error || text || `HTTP ${response.statusCode}`)); }
          catch { reject(new Error(text || `HTTP ${response.statusCode}`)); }
          return;
        }
        try { resolve(JSON.parse(text)); }
        catch { reject(new Error('The model service returned an unreadable response.')); }
      });
    });
    request.setTimeout(15_000, () => request.destroy(new Error('The model service did not respond within 15 seconds.')));
    request.on('error', reject);
  });
}

let output: vscode.OutputChannel | undefined;

function gridOutput(context: vscode.ExtensionContext): vscode.OutputChannel {
  if (!output) {
    output = vscode.window.createOutputChannel('SysML Grids');
    context.subscriptions.push(output);
  }
  return output;
}

function shortReason(error: unknown): string {
  return (error instanceof Error ? error.message : String(error)).replace(/\s+/g, ' ').slice(0, 240);
}

interface OpenPanel {
  panel: vscode.WebviewPanel;
  refresh: () => Promise<void>;
}
const panels = new Map<string, OpenPanel>();

export async function openGrid(context: vscode.ExtensionContext, baseUrl: string, kind: GridKind): Promise<void> {
  const address = new URL('/api/grid', baseUrl);
  address.searchParams.set('kind', kind);
  const label = kind === 'matrix' ? 'requirement satisfaction matrix' : 'model element table';
  const key = `${address.origin}/${kind}`;
  const existing = panels.get(key);
  if (existing) {
    existing.panel.reveal();
    await existing.refresh();
    return;
  }
  const log = gridOutput(context);
  const notify = async (operation: string, error: unknown): Promise<void> => {
    log.appendLine(`${operation}: ${error instanceof Error ? error.stack || error.message : String(error)}`);
    const answer = await vscode.window.showErrorMessage(`${operation}: ${shortReason(error)} Open SysML Grids Output for details.`, 'Show Output');
    if (answer === 'Show Output') log.show(true);
  };
  let data: GridData;
  try {
    data = parseGridData(await readJson(address));
  } catch (error) {
    await notify(`Cannot open the ${label}`, error);
    return;
  }
  const panel = vscode.window.createWebviewPanel('sysml.grid', data.title, vscode.ViewColumn.Beside,
    { enableScripts: true, retainContextWhenHidden: true });
  panel.webview.html = renderGridHtml(data, kind);
  context.subscriptions.push(panel);
  let disposed = false;
  let refreshing = false;
  const refresh = async (): Promise<void> => {
    if (refreshing || disposed) return;
    refreshing = true;
    try {
      const next = parseGridData(await readJson(address));
      if (disposed) return;
      data = next;
      panel.title = data.title;
      await panel.webview.postMessage({ type: 'data', data });
    } catch (error) {
      log.appendLine(`Cannot refresh the ${label}: ${error instanceof Error ? error.stack || error.message : String(error)}`);
      if (!disposed) await panel.webview.postMessage({ type: 'error', message: `Cannot refresh the ${label}: ${shortReason(error)} The previous data is unchanged. Check the model service, then try Refresh again.` });
    } finally {
      refreshing = false;
    }
  };
  panels.set(key, { panel, refresh });
  panel.onDidDispose(() => { disposed = true; panels.delete(key); });
  const messages = panel.webview.onDidReceiveMessage(async (message: unknown) => {
    if (!message || typeof message !== 'object' || disposed) return;
    const action = message as { type?: string; index?: number };
    if (action.type === 'refresh') {
      await refresh();
    } else if (action.type === 'export') {
      try {
        const snapshot = data;
        const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
        const target = await vscode.window.showSaveDialog({
          title: 'Export all rows as CSV', filters: { CSV: ['csv'] },
          saveLabel: 'Export all rows',
          defaultUri: folder ? vscode.Uri.joinPath(folder, kind === 'matrix' ? 'requirement-satisfaction.csv' : 'model-elements.csv') : undefined,
        });
        if (!target || disposed) return;
        await vscode.workspace.fs.writeFile(target, Buffer.from(snapshot.csv, 'utf8'));
        const open = await vscode.window.showInformationMessage(`Exported all ${snapshot.rows.length} rows to ${target.fsPath}.`, 'Open');
        if (open === 'Open') await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(target));
      } catch (error) {
        await notify('Cannot export CSV', error);
      }
    } else if (action.type === 'source' && Number.isInteger(action.index)
        && action.index! >= 0 && action.index! < data.rowKeys.length) {
      const sourceKey = data.rowKeys[action.index!];
      if (!sourceKey) return;
      try {
        const sourceUrl = new URL('/api/source', baseUrl);
        sourceUrl.searchParams.set('key', sourceKey);
        const value = await readJson(sourceUrl);
        if (!value || typeof value !== 'object') throw new Error('The model service returned unreadable source information.');
        const source = value as { ok?: boolean; error?: string; path?: string; file?: string; line?: number };
        const file = source.path || source.file;
        if (source.ok !== true || typeof file !== 'string' || !file || !Number.isInteger(source.line) || source.line! < 1) {
          throw new Error(source.error || 'This row has no source declaration. Refresh the view and try again.');
        }
        const document = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
        const position = new vscode.Position(source.line! - 1, 0);
        const editor = await vscode.window.showTextDocument(document, { preserveFocus: true,
          selection: new vscode.Range(position, position) });
        editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenterIfOutsideViewport);
      } catch (error) {
        await notify('Cannot open the source declaration', error);
      }
    }
  });
  panel.onDidDispose(() => messages.dispose());
}
