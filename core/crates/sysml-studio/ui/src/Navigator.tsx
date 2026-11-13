import { useState } from 'react';
import { t } from './i18n';

/** One line of `/api/outline`. Mirrors `document::OutlineEntry`. */
export interface OutlineEntry {
  key: string;
  label: string;
  kind: string;
  is_def: boolean;
  line: number;
  children: OutlineEntry[];
}

/**
 * The model's table of contents.
 *
 * Built from the source tree, not from the current view: `gv` drops packages and `pv` drops
 * features, so a tree derived from the picture would show a different model depending on which
 * view is open. Clicking an entry asks the canvas to show that element and centres on it.
 */
export function Navigator({
  entries,
  selection,
  onLocate,
}: {
  entries: OutlineEntry[];
  selection: string[];
  onLocate: (key: string) => void;
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const chosen = new Set(selection);

  const toggle = (key: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const rows: React.ReactNode[] = [];
  const walk = (list: OutlineEntry[], depth: number) => {
    for (const entry of list) {
      const kids = entry.children.length > 0;
      const shut = collapsed.has(entry.key);
      rows.push(
        <div
          key={entry.key}
          className={`entry${chosen.has(entry.key) ? ' on' : ''}`}
          style={{ paddingLeft: 6 + depth * 12 }}
          onClick={() => onLocate(entry.key)}
          title={`${entry.kind} · ${entry.key}`}
        >
          <button
            className={`twisty${kids ? '' : ' empty'}`}
            onClick={(event) => { event.stopPropagation(); toggle(entry.key); }}
            tabIndex={-1}
          >
            {kids ? (shut ? '▸' : '▾') : ''}
          </button>
          <span className={entry.is_def ? 'tname def' : 'tname'}>{entry.label}</span>
        </div>,
      );
      if (kids && !shut) walk(entry.children, depth + 1);
    }
  };
  walk(entries, 0);

  return (
    <aside className="navigator">
      <div className="panel-head">{t('Model')}</div>
      <div className="panel-scroll">{rows}</div>
    </aside>
  );
}
