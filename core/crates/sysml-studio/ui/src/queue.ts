import type { CanvasDocument } from './types';

export type QueueKind = 'unrecorded' | 'unapproved' | 'stale-verdict' | 'problem';

/** One thing a reviewer has to deal with, in the order they should deal with it. */
export interface QueueItem {
  /** Stable across reloads: the panel keeps the active row by this. */
  id: string;
  kind: QueueKind;
  /** Qualified name of the element the row is about. */
  key: string;
  /** One line, the thing itself. */
  title: string;
  /** The evidence: a value, a message, a fingerprint pair. */
  detail: string;
  /** For a decision row: which record it acts on. */
  decisionId?: string;
  /** For a stale verdict: which requirement to re-judge. */
  requirement?: string;
}

/**
 * The review queue.
 *
 * Ordered by what blocks the gate, not by severity: an unrecorded value fails the provenance
 * check outright, an unapproved assumption is a claim nobody has accepted, a stale verdict is a
 * record about a model that no longer exists, and a diagnostic is the checker talking. Within a
 * kind the document order is kept, so the queue does not shuffle between reloads.
 *
 * Duplicates are dropped on purpose: an unrecorded value *is* the PROV001 diagnostic about it.
 * Listing both would make one fact look like two, and the second row would have no action.
 */
export function buildQueue(doc: CanvasDocument): QueueItem[] {
  const unrecorded = new Set<string>();
  const unapprovedElements = new Set<string>();

  const items: QueueItem[] = [];

  for (const value of doc.values) {
    if (!value.origin) {
      unrecorded.add(value.key);
      items.push({
        id: `unrecorded:${value.key}`,
        kind: 'unrecorded',
        key: value.key,
        title: value.key,
        detail: value.text,
        decisionId: value.decision_id ?? undefined,
      });
    } else if (value.origin === 'assumed' && !value.approved_by) {
      unapprovedElements.add(value.key);
      items.push({
        id: `unapproved:${value.decision_id ?? value.key}`,
        kind: 'unapproved',
        key: value.key,
        title: value.key,
        detail: value.why || value.text,
        decisionId: value.decision_id ?? undefined,
      });
    }
  }

  for (const judgement of doc.judgements) {
    if (!judgement.stale) continue;
    items.push({
      id: `stale:${judgement.key}`,
      kind: 'stale-verdict',
      key: judgement.key,
      title: judgement.key,
      detail: `${judgement.verdict} · judged against ${judgement.judged_against} · model is ${doc.ledger_fingerprint}`,
      requirement: judgement.key,
    });
  }

  for (const issue of doc.issues) {
    // The value rows already carry these two facts, with an action attached.
    if (issue.at && (unrecorded.has(issue.at) || unapprovedElements.has(issue.at))) continue;
    items.push({
      id: `problem:${issue.code}:${issue.at}:${issue.message}`,
      kind: 'problem',
      key: issue.at,
      title: `${issue.code} ${issue.message}`,
      detail: issue.at,
    });
  }

  return items;
}

/** How many rows of each kind, for the tab label. */
export function queueCounts(items: QueueItem[]): Record<QueueKind, number> {
  const counts: Record<QueueKind, number> = {
    unrecorded: 0,
    unapproved: 0,
    'stale-verdict': 0,
    problem: 0,
  };
  for (const item of items) counts[item.kind] += 1;
  return counts;
}
