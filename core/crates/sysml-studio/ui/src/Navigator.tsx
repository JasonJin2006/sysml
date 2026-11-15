import { useState } from 'react';
import { FileIcon } from '@radix-ui/react-icons';
import { t } from './i18n';
import { groupFiles } from './files';
import type { ViewEntry } from './types';
import { supportsElementScope } from './diagram';

/** One line of `/api/outline`. Mirrors `document::OutlineEntry`. */
export interface OutlineEntry {
  key: string;
  label: string;
  kind: string;
  is_def: boolean;
  line: number;
  children: OutlineEntry[];
}

/** A built-in projection, as a row in the view list. */
export interface ViewChoice {
  key: string;
  /** i18n key of the short label. */
  label: string;
  /** i18n key of the long description used as the tooltip. */
  title: string;
}

function flatten(list: OutlineEntry[]): OutlineEntry[] {
  return list.flatMap((entry) => [entry, ...flatten(entry.children)]);
}

/**
 * The single navigation column.
 *
 * Views, the views the model declares, the files and the outline are four
 * groups in one list. The view tabs used to live in the top bar as well, which
 * gave two entries for the same thing and made the short keys look like they
 * named one concept. A projection and a declared view are different things,
 * so they keep separate group headings here.
 */
export function Navigator({
  entries,
  files,
  current,
  selection,
  onLocate,
  onOpenFile,
  views,
  view,
  onView,
  declaredViews,
  modelView,
  onModelView,
  scope,
  busy = false,
  onWholeModel,
}: {
  entries: OutlineEntry[];
  /** Every file the project is made of, as the server names them. */
  files: string[];
  /** The file the editor holds, so its row can be marked. */
  current: string | null;
  selection: string[];
  onLocate: (key: string) => void;
  onOpenFile: (path: string) => void;
  views: readonly ViewChoice[];
  view: string;
  onView: (key: string) => void;
  declaredViews: readonly ViewEntry[];
  modelView: string | null;
  onModelView: (key: string | null) => void;
  scope?: string | null;
  busy?: boolean;
  onWholeModel?: () => void;
}) {
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const chosen = new Set(selection);
  const needle = query.trim().toLowerCase();

  const toggle = (key: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const row = (entry: OutlineEntry, depth: number) => {
    const kids = entry.children.length > 0;
    const shut = collapsed.has(entry.key);
    return (
      <div
        key={entry.key}
        className={`entry${chosen.has(entry.key) ? ' on' : ''}`}
        style={{ paddingLeft: 14 + depth * 14 }}
        onClick={() => onLocate(entry.key)}
        title={`${entry.kind} ${entry.key}`}
      >
        <button
          className={`twisty${kids ? '' : ' empty'}`}
          onClick={(event) => { event.stopPropagation(); toggle(entry.key); }}
          tabIndex={-1}
        >
          {kids ? (shut ? '▸' : '▾') : ''}
        </button>
        <span className={entry.is_def ? 'tname def' : 'tname'}>{entry.label}</span>
      </div>
    );
  };

  const rows: React.ReactNode[] = [];
  if (needle) {
    // A search is flat by nature: an indented tree whose parents do not match
    // reads as broken hierarchy, so matching entries are listed on one level.
    for (const entry of flatten(entries)) {
      if (entry.label.toLowerCase().includes(needle)) rows.push(row(entry, 0));
    }
  } else {
    const walk = (list: OutlineEntry[], depth: number) => {
      for (const entry of list) {
        rows.push(row(entry, depth));
        if (entry.children.length > 0 && !collapsed.has(entry.key)) {
          walk(entry.children, depth + 1);
        }
      }
    };
    walk(entries, 0);
  }

  const groups = groupFiles(files).map((group) => ({
    ...group,
    files: group.files.filter((file) => !needle || file.name.toLowerCase().includes(needle)),
  })).filter((group) => group.files.length > 0);

  const viewRow = (key: string, short: string, label: string, title: string, active: boolean, run: () => void, disabled = false) => (
    <div key={key} className={`view-entry${active ? ' on' : ''}`} title={title}
      role="button" tabIndex={disabled ? -1 : 0} aria-disabled={disabled}
      onClick={() => { if (!disabled) run(); }}
      onKeyDown={(event) => { if (!disabled && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); run(); } }}>
      <span className="view-key">{short}</span>
      <span>{label}</span>
    </div>
  );

  return (
    <aside className="navigator">
      <div className="panel-head">
        <span>{t('Navigation')}</span>
      </div>
      <div className="nav-search-wrap">
        <label className="nav-search">
          <input
            value={query}
            placeholder={t('Search the model')}
            onChange={(event) => setQuery(event.target.value)}
            aria-label={t('Search the model')}
          />
        </label>
      </div>
      <div className="panel-scroll">
        <div className="nav-group-title">{t('Projection')}</div>
        {(scope || modelView) && <div className="nav-scope">
          <span>{modelView ? `${t('Model-declared view')}: ${modelView}` : `${t('Element subtree')}: ${scope}`}</span>
          <button className="diagram-options-toggle" disabled={busy} onClick={onWholeModel}>{t('Show whole model')}</button>
        </div>}
        {views.map((choice) => viewRow(
          choice.key,
          choice.key.toUpperCase(),
          t(choice.label),
          modelView || (scope && !supportsElementScope(choice.key)) ? t('Choose Whole model before using this projection.') : t(choice.title),
          choice.key === view && modelView === null,
          () => onView(choice.key),
          busy || Boolean(modelView) || Boolean(scope && !supportsElementScope(choice.key)),
        ))}

        <div className="nav-divider" />
        <div className="nav-group-title">{t('Views the model declares')}</div>
        {declaredViews.map((entry) => viewRow(
          entry.key,
          'VIEW',
          entry.name,
          entry.projection
            ? `${entry.key} ${entry.target} ${entry.projection}`
            : `${entry.key} ${t('This model-declared view has no supported projection.')}`,
          modelView === entry.key,
          () => onModelView(entry.key),
          busy || !entry.projection,
        ))}

        <div className="nav-divider" />
        <div className="nav-group-title">{t('Files')}</div>
        {groups.map((group) => (
          <div key={group.dir || '.'} className="file-group">
            {group.dir && <div className="file-dir">{group.dir}</div>}
            {group.files.map((file) => (
              <div
                key={file.path}
                className={`entry file${file.path === current ? ' on' : ''}`}
                title={file.path}
                onClick={() => onOpenFile(file.path)}
              >
                <span className="file-icon"><FileIcon /></span>
                <span className="tname">{file.name}</span>
              </div>
            ))}
          </div>
        ))}

        <div className="nav-divider" />
        <div className="nav-group-title">{t('Outline')}</div>
        {rows}
      </div>
    </aside>
  );
}
