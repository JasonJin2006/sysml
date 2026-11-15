/**
 * Interface theme.
 *
 * The palette is one set of CSS variables selected by `data-theme` on <html>.
 * Radix Themes and Monaco each keep their own palette, so both are told when
 * the choice changes -- see main.tsx and Editor.tsx.
 */
import { useEffect, useState } from 'react';
import { styleTheme } from './style';

export type ThemeName = 'dark' | 'light';

const STORAGE_KEY = 'sysml-studio.theme';
export const THEME_EVENT = 'sysml-theme-change';

function stored(): ThemeName {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'light' ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

/**
 * Whether the host decides the palette, rather than the person.
 *
 * In an editor the answer is always the host's: the window is already light or dark, and a canvas
 * in the other scheme reads as a broken panel. That choice must not be written to `localStorage`
 * either — the storage is per origin, and the standalone studio is served from the same origin, so
 * a panel adopting the editor's light theme would leave the next browser tab light as well, and a
 * dark window after it would open light because a saved preference outranked the fragment.
 */
let adopted = false;

let current: ThemeName = stored();

export function theme(): ThemeName {
  return current;
}

/** Set the attribute before the first paint, or the page flashes in the wrong palette. */
export function applyTheme(): void {
  document.documentElement.dataset.theme = current;
  document.documentElement.style.colorScheme = current;
  // The boxes take their colours from a table in TypeScript, not from CSS, so the table has to be
  // told which palette is current. Keeping it here means a theme change cannot leave the boxes on
  // the previous one.
  styleTheme(current);
}

export function setTheme(next: ThemeName): void {
  current = next;
  applyTheme();
  // An adopted palette is the host's, not a preference: saving it would make the standalone studio
  // open in whatever the editor happened to be.
  if (!adopted) {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* the choice lasts for this session only */
    }
  }
  window.dispatchEvent(new Event(THEME_EVENT));
}

/**
 * Take the palette from the host, for as long as this page exists.
 *
 * Called before the first paint, so the first frame is already right. From here on the toolbar's
 * theme toggle still works — a person looking at the canvas may want it light while the window is
 * dark — but nothing is saved and the host's next message wins again.
 */
export function adoptTheme(next: ThemeName): void {
  adopted = true;
  setTheme(next);
}

export function useTheme(): [ThemeName, (next: ThemeName) => void] {
  const [value, setValue] = useState<ThemeName>(theme);
  useEffect(() => {
    const onChange = () => setValue(theme());
    window.addEventListener(THEME_EVENT, onChange);
    return () => window.removeEventListener(THEME_EVENT, onChange);
  }, []);
  return [value, setTheme];
}
