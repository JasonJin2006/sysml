/**
 * What the answer to a poll means for the picture on screen.
 *
 * The page asks the server once a second for the digest of the source files and compares it with
 * the digest of the document it is showing. An answer means one of three things, and they are kept
 * apart here so the timing rule has a test:
 *
 * - `ignore`: a load started after the poll was sent. That load reads the files at least as late
 *   as the poll did and records its own digest, so acting on the poll would reload a picture that
 *   is already current and announce the person's own edit as a change made somewhere else.
 * - `unchanged`: the files are what the picture shows, or nothing is shown yet.
 * - `changed`: the files differ from the picture, so it has to be reloaded.
 */
export type PollVerdict = 'ignore' | 'unchanged' | 'changed';

export function judgePoll(poll: {
  /** The digest of the document on screen; null before the first one arrives. */
  shown: string | null;
  /** The digest the server answered. */
  answered: string;
  /** Whether a load started after this poll was sent. */
  superseded: boolean;
}): PollVerdict {
  if (poll.superseded) return 'ignore';
  if (poll.shown === null || poll.shown === poll.answered) return 'unchanged';
  return 'changed';
}
