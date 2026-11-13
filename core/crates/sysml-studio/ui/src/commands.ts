/**
 * The command palette's matching, kept away from React.
 *
 * A test runner that imports a `.tsx` module has to understand JSX; this file has none, so the
 * rules can be tested on their own. The component in `Palette.tsx` uses these.
 */

export interface Command {
  /** Stable id, also the i18n key for its title. */
  id: string;
  /** The keys to press, shown to the right of the title. Empty when there is no shortcut. */
  keys?: string;
  run: () => void;
  /** Greyed out, with the reason in its title, when it cannot run right now. */
  disabled?: boolean;
  reason?: string;
}

/** Whether every character of `query` appears in `text`, in order. */
export function matches(query: string, text: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  const hay = text.toLowerCase();
  let at = 0;
  for (const ch of needle) {
    if (ch === ' ') continue;
    const found = hay.indexOf(ch, at);
    if (found < 0) return false;
    at = found + 1;
  }
  return true;
}

/**
 * The commands a query keeps, in the order they were given.
 *
 * Both the title and the id are searched: the title is what a person reads, and the id is what a
 * shortcut-minded typist reaches for.
 */
export function filterCommands(
  commands: readonly Command[],
  query: string,
  title: (command: Command) => string,
): Command[] {
  return commands.filter(
    (command) => matches(query, title(command)) || matches(query, command.id),
  );
}
