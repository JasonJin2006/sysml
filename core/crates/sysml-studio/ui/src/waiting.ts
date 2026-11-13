/**
 * What the inspector shows when nothing is selected.
 *
 * Only the facts that are still *waiting for a person*: a value with no record, an assumption
 * nobody approved, a question nobody answered, a verdict computed against an older model. A count
 * of elements or of errors belongs to the status bar and the problems panel; repeating it here
 * would make the panel say the same thing twice and train the reader to skip it.
 */

export interface Waiting {
  count: number;
  /** The i18n key, not the text: the panel translates it at render time. */
  key: string;
}

export interface SummaryCounts {
  values_without_record?: number;
  assumptions_unapproved?: number;
  open_questions?: number;
  judgements_stale?: number;
}

/**
 * The outstanding facts, largest first, with the zeroes dropped.
 *
 * Dropping them is the point: a panel of four boxes reading `0` looks like four things that were
 * checked, and the one box that is not zero gets lost among them.
 */
export function waitingOf(summary: SummaryCounts): Waiting[] {
  const all: Waiting[] = [
    { count: summary.values_without_record ?? 0, key: 'values with no record' },
    { count: summary.assumptions_unapproved ?? 0, key: 'unapproved assumptions' },
    { count: summary.open_questions ?? 0, key: 'open questions' },
    { count: summary.judgements_stale ?? 0, key: 'stale verdicts' },
  ];
  return all
    .filter((entry) => entry.count > 0)
    .sort((a, b) => b.count - a.count);
}
