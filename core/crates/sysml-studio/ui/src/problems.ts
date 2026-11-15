/**
 * Filtering the problems list.
 *
 * A model with two hundred warnings and three errors is read by hiding the warnings, and a model
 * with a hundred instances of one rule is read by hiding that rule. Both filters are here rather
 * than in the component so the rules can be tested: an empty result has to be distinguishable
 * from "there were no problems at all".
 */

export interface Problem {
  severity: 'error' | 'warning' | 'info';
  code: string;
  message: string;
  at: string;
  line: number;
}

export type SeverityFilter = 'all' | 'error' | 'warning' | 'info';

export interface ProblemFilter {
  severity: SeverityFilter;
  /** A rule code, or `null` for every code. */
  code: string | null;
}

export const NO_FILTER: ProblemFilter = { severity: 'all', code: null };

/** The problems a filter keeps, in their original order. */
export function filterProblems(problems: readonly Problem[], filter: ProblemFilter): Problem[] {
  return problems.filter((problem) => {
    if (filter.severity !== 'all' && problem.severity !== filter.severity) return false;
    if (filter.code !== null && problem.code !== filter.code) return false;
    return true;
  });
}

/**
 * How many problems there are of each rule code, largest first.
 *
 * The list is what the code filter offers, so it is built from the *unfiltered* set: a code that
 * the severity filter currently hides must still be reachable, or choosing "errors only" would
 * make every warning-only code disappear from the menu that could bring it back.
 */
export function codeCounts(problems: readonly Problem[]): Array<{ code: string; count: number }> {
  const counts = new Map<string, number>();
  for (const problem of problems) {
    counts.set(problem.code, (counts.get(problem.code) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([code, count]) => ({ code, count }))
    .sort((a, b) => b.count - a.count || a.code.localeCompare(b.code));
}

/** Counts for the severity filters; informational notices are not warnings. */
export function severityCounts(problems: readonly Problem[]): Record<SeverityFilter, number> {
  const counts = { all: problems.length, error: 0, warning: 0, info: 0 };
  for (const problem of problems) counts[problem.severity] += 1;
  return counts;
}
