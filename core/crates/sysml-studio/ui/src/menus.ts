/**
 * The toolbar menus' contents, as data.
 *
 * Building the item lists here rather than inline keeps them testable: a toggle that forgets to
 * report `checked`, or a group whose first item has no separator, is a rendering bug that a test on
 * plain objects can catch.
 */

import type { MenuItem } from './Menu';

export interface DisplayPart {
  part: string;
  /** The i18n key of the item. */
  title: string;
}

/**
 * The display menu: what a box shows, then the interface language.
 *
 * The language items start a new group, because they change the interface rather than the picture;
 * a menu that ran them together would suggest they were the same kind of choice.
 */
export function displayMenu(
  parts: readonly DisplayPart[],
  shown: Record<string, boolean>,
  languages: readonly string[],
  current: string,
  toggle: (part: string) => void,
  choose: (code: string) => void,
): MenuItem[] {
  return [
    ...parts.map((part) => ({
      id: part.title,
      checked: shown[part.part] === true,
      run: () => toggle(part.part),
    })),
    ...languages.map((code, index) => ({
      id: code === 'en' ? 'English' : '中文',
      checked: code === current,
      startsGroup: index === 0,
      run: () => choose(code),
    })),
  ];
}

/** The marks menu: show the differences, or take the current picture as the baseline. */
export function marksMenu(
  showing: boolean,
  toggle: () => void,
  clear: () => void,
): MenuItem[] {
  return [
    { id: 'Show what changed since this view was opened', checked: showing, run: toggle },
    { id: 'Take the current picture as the new baseline', run: clear },
  ];
}
