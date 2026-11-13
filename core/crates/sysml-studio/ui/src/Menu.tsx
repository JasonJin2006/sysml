import { useEffect, useRef, useState } from 'react';

/**
 * A toolbar menu.
 *
 * The bar cannot hold everything without becoming a row of thirty buttons, and a menu is the
 * shape that keeps the frequent controls in reach while the rest stay one click away. It closes
 * on Escape, on a click outside, and after an item runs — the last is what makes a menu feel
 * finished rather than sticky.
 */

export interface MenuItem {
  /** The i18n key, also the React key. */
  id: string;
  /** Shown to the right of the title. */
  keys?: string;
  /** A tick on the left, for an item that is a toggle. */
  checked?: boolean;
  /** A line above this item, when it starts a group. */
  startsGroup?: boolean;
  disabled?: boolean;
  reason?: string;
  run: () => void;
}

export interface MenuProps {
  /** The i18n key of the button that opens it. */
  label: string;
  items: MenuItem[];
  /** Rendered by the caller, so this file holds no translation table. */
  t: (key: string) => string;
}

export function Menu({ label, items, t }: MenuProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className="menu" ref={root}>
      <button
        className={`plain menu-button${open ? ' on' : ''}`}
        aria-haspopup="menu"
        aria-expanded={open}
        title={t(label)}
        onClick={() => setOpen((was) => !was)}
      >
        {t(label)}
      </button>
      {open && (
        <ul className="menu-list" role="menu">
          {items.map((item) => (
            <li
              key={item.id}
              role="menuitem"
              aria-checked={item.checked}
              className={`menu-item${item.startsGroup ? ' group' : ''}${item.disabled ? ' off' : ''}`}
              title={item.disabled ? item.reason : undefined}
              onClick={() => {
                if (item.disabled) return;
                setOpen(false);
                item.run();
              }}
            >
              <span className="menu-tick">{item.checked ? '✓' : ''}</span>
              <span className="menu-title">{t(item.id)}</span>
              {item.keys && <span className="menu-keys">{item.keys}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
