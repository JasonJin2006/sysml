import type { CanvasDocument, SourceRange } from './types';
import { t, type Lang } from './i18n.ts';
import type { Folded } from './folding';

function isAbort(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}

function responseReason(text: string, status: number): string {
  if (!text.trim() || /^\s*</.test(text)) {
    return t('The model service returned HTTP {status}.', { status });
  }
  const reason = text.replace(/\s+/g, ' ').trim();
  return reason.length > 200 ? `${reason.slice(0, 197)}…` : reason;
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  let text: string;
  try {
    response = await fetch(path, init);
    text = await response.text();
  } catch (error) {
    if (isAbort(error)) throw error;
    console.warn('Model service request failed', { path, error });
    throw new Error(t('Cannot reach the model service. Reopen the diagram and try again.'));
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    if (response.ok) {
      console.warn('Model service returned invalid JSON', { path, status: response.status, body: text });
      throw new Error(t('The model service returned an invalid response. Reopen the diagram and try again.'));
    }
  }
  if (!response.ok) {
    console.warn('Model service rejected a request', { path, status: response.status, body: text });
    const error = parsed && typeof parsed === 'object' && 'error' in parsed
      ? (parsed as { error: unknown }).error : undefined;
    throw new Error(responseReason(typeof error === 'string' ? error : text, response.status));
  }
  return parsed as T;
}

export function api<T>(path: string, body?: unknown): Promise<T> {
  return requestJson(path, body === undefined ? undefined : {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

/** Take back the last edit. `undone: false` means there was nothing to take back. */
export function undoEdit(): Promise<{ ok: boolean; undone: boolean }> {
  return api('/api/undo', {});
}

/** Put back the last edit an undo took away. `redone: false` means there was nothing to put back. */
export function redoEdit(): Promise<{ ok: boolean; redone: boolean }> {
  return api('/api/redo', {});
}

export function documentUrl(
  view: string,
  mode: string,
  only?: string | null,
  filter?: string,
  scope?: string | null,
  depth?: number | null,
  layout?: 'nested' | 'tree',
  folded?: Folded,
): string {
  const base = `/api/document?view=${view}&mode=${mode}`;
  const scoped = only ? `${base}&only=${encodeURIComponent(only)}` : base;
  const filtered = filter && filter.trim() !== '' ? `${scoped}&filter=${encodeURIComponent(filter)}` : scoped;
  const element = scope ? `${filtered}&scope=${encodeURIComponent(scope)}` : filtered;
  const withDepth = depth !== undefined && depth !== null ? `${element}&depth=${depth}` : element;
  const styled = layout ? `${withDepth}&layout=${layout}` : withDepth;
  return folded && Object.keys(folded).length > 0 ? `${styled}&folded=${encodeURIComponent(JSON.stringify(folded))}` : styled;
}

export function fetchDocument(
  view: string,
  mode: string,
  only?: string | null,
  filter?: string,
  signal?: AbortSignal,
  scope?: string | null,
  depth?: number | null,
  layout?: 'nested' | 'tree',
  folded?: Folded,
): Promise<CanvasDocument> {
  return requestJson<CanvasDocument>(documentUrl(view, mode, only, filter, scope, depth, layout, folded), { signal }).catch((error) => {
    if (isAbort(error)) throw error;
    const reason = (error instanceof Error ? error.message : String(error)).replace(/[.!?。！？]$/, '');
    console.warn('Document request failed', { view, mode, only, filter, error });
    throw new Error(t('Cannot update the diagram: {reason}. The previous diagram is unchanged.', { reason }));
  });
}

/**
 * The digest of the source files as they stand on disk.
 *
 * This is what the one-second poll asks. It is the `fingerprint` a document carries, without the
 * document: the server re-reads the files and digests them, and parses nothing.
 */
export function fetchFingerprint(): Promise<string> {
  return api<{ fingerprint: string }>('/api/fingerprint').then((answer) => answer.fingerprint);
}

export function fetchSource(key: string): Promise<SourceRange> {
  return requestJson<SourceRange>(`/api/source?key=${encodeURIComponent(key)}`)
    .catch((error) => ({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      text: '', path: '', line: 0, endLine: 0, nameStart: 0, nameEnd: 0,
    }));
}

export interface EditResult {
  ok?: boolean;
  error?: string;
  stale?: boolean;
  touched?: string[];
}

// Edits carry the fingerprint of the document the person is looking at. The server verifies it
// against every buffer before writing anything, so an edit aimed at text that has since changed
// is refused as `stale` instead of being applied to a different picture.
export function edit(path: string, body: Record<string, unknown>): Promise<EditResult> {
  return api<EditResult>(path, body);
}

/** What the model is waiting for: the same ladder `sysml next` prints. */
export type Plan = {
  stage: string;
  number: number;
  label: string;
  established: string;
  waiting: string[];
  command: string | null;
  /** The elements that hold the model at this stage, by qualified name — the canvas keys boxes by
   *  the same name, so a chip can jump to one. */
  targets: string[];
  /** Every element that holds the model, by qualified name, with why. The strip shows five; the
   *  inspector asks about one. */
  blocking: Record<string, string>;
  counts: Record<string, number>;
};

export function fetchPlan(language: Lang): Promise<Plan> {
  return api<Plan>(`/api/next?lang=${language}`);
}

/** One buffer, as the editor holds it. */
export type FileText = {
  ok: boolean;
  path: string;
  index: number;
  text: string;
  files: string[];
  error?: string;
};

export function fetchFile(path: string): Promise<FileText> {
  return fetch(`/api/file?path=${encodeURIComponent(path)}`)
    .then((r) => r.json())
    .catch(() => ({ ok: false, path, index: -1, text: '', files: [], error: t('Could not read the file') }));
}

/**
 * Save a whole buffer.
 *
 * The fingerprint is what makes this safe: the server refuses to write a buffer whose file has
 * changed since the page read it, so an editor left open overnight cannot overwrite someone's
 * morning's work.
 */
export function saveFile(
  path: string,
  text: string,
  fingerprint: string | null,
): Promise<{ ok?: boolean; error?: string; stale?: boolean }> {
  return fetch('/api/file', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ path, text, fingerprint }),
  })
    .then((r) => r.json())
    .catch(() => ({ ok: false, error: t('Could not save the file') }));
}
