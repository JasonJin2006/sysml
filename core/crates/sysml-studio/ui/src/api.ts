import type { CanvasDocument, SourceRange } from './types';
import { t, type Lang } from './i18n';

// Every route answers JSON. A non-OK response carries `{error}` or plain text; both are turned
// into an Error here so a caller never has to decide which shape it got.
export async function api<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, body === undefined ? undefined : {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error(text || `HTTP ${response.status}`);
  }
  if (!response.ok) {
    const message = (parsed as { error?: string }).error ?? text;
    throw new Error(message);
  }
  return parsed as T;
}

/** Take back the last edit. `undone: false` means there was nothing to take back. */
export function undoEdit(): Promise<{ ok: boolean; undone: boolean }> {
  return api('/api/undo', {});
}

/** Put back the last edit an undo took away. `redone: false` means there was nothing to put back. */
export function redoEdit(): Promise<{ ok: boolean; redone: boolean }> {
  return api('/api/redo', {});
}

export function documentUrl(view: string, mode: string, only?: string | null, filter?: string): string {
  const base = `/api/document?view=${view}&mode=${mode}`;
  // `only` names a view the model declares; its exposes then decide what the picture contains.
  const scoped = only ? `${base}&only=${encodeURIComponent(only)}` : base;
  // `filter` narrows to what an annotation says, spelled the way the CLI spells it.
  return filter && filter.trim() !== '' ? `${scoped}&filter=${encodeURIComponent(filter)}` : scoped;
}

export function fetchDocument(
  view: string,
  mode: string,
  only?: string | null,
  filter?: string,
  signal?: AbortSignal,
): Promise<CanvasDocument> {
  return fetch(documentUrl(view, mode, only, filter), { signal }).then(async (response) => {
    const text = await response.text();
    if (!response.ok) throw new Error(text);
    return JSON.parse(text) as CanvasDocument;
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
  return fetch(`/api/source?key=${encodeURIComponent(key)}`)
    .then((r) => r.json())
    .catch(() => ({ ok: false, error: t('Could not read the file') }));
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
