/** Explicit per-node choices: true hides descendants, false opens beyond the depth limit. */
export type Folded = Record<string, boolean>;

export function foldingKey(files: string[], view: string, scope: string | null, only: string | null, filter: string): string {
  return `sysml-studio.folding.v1:${JSON.stringify([files, view, scope, only, filter.trim()])}`;
}

export function readFolded(text: string | null): Folded {
  try {
    const value: unknown = JSON.parse(text ?? '{}');
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter(([key, folded]) => key !== '' && typeof folded === 'boolean'));
  } catch {
    return {};
  }
}
