import type { ViewKey } from './types';

export interface DiagramSelection {
  view: ViewKey;
  only: string | null;
  scope: string | null;
  depth: number | null;
  layout: 'nested' | 'tree';
}

export function supportsElementScope(view: string): boolean {
  return view === 'gv' || view === 'stv' || view === 'afv';
}

/** Changing the projection never silently widens an element selection. */
export function projectSelection(current: DiagramSelection, view: ViewKey): DiagramSelection | null {
  if (current.only || (current.scope && !supportsElementScope(view))) return null;
  return {
    ...current, view, only: null,
    depth: supportsElementScope(view) ? current.depth : null,
  };
}

export function wholeModel(current: DiagramSelection): DiagramSelection {
  return { ...current, only: null, scope: null, depth: null };
}
