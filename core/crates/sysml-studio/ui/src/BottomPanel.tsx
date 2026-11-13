import type { CanvasDocument } from './types';
import { SourceView } from './SourceView';
import { t } from './i18n';
import { QueuePanel } from './QueuePanel';
import type { QueueItem } from './queue';
import type { Submit } from './Inspector';

export type BottomTab = 'problems' | 'queue' | 'values' | 'questions' | 'source';

/**
 * The tabs an IDE puts along the bottom: what is wrong, what has no source, what has no answer,
 * and the text the picture came from.
 *
 * They share one panel rather than sitting in the inspector because they are about the model, not
 * about the selected box, and because a list that scrolls sideways steals height from the canvas
 * only once.
 */
export function BottomPanel({
  doc,
  selected,
  tab,
  setTab,
  collapsed,
  setCollapsed,
  onLocate,
  onLocateIfFound,
  onLocateLine,
  queue,
  queueActive,
  setQueueActive,
  onJudge,
  submit,
  toast,
  reviewer,
  setReviewer,
}: {
  doc: CanvasDocument | null;
  selected: string | null;
  tab: BottomTab;
  setTab: (tab: BottomTab) => void;
  collapsed: boolean;
  setCollapsed: (collapsed: boolean) => void;
  onLocate: (key: string) => void;
  /** Select an element when the view draws it; false when it does not. */
  onLocateIfFound: (key: string) => boolean;
  /** Put the editor cursor on a line, when the diagnostic names no element. */
  onLocateLine: (line: number) => void;
  queue: QueueItem[];
  queueActive: string | null;
  setQueueActive: (id: string | null) => void;
  onJudge: (requirement: string) => Promise<void>;
  submit: Submit;
  toast: (message: string, bad?: boolean) => void;
  reviewer: string;
  setReviewer: (name: string) => void;
}) {
  const counts: Array<[BottomTab, string, number]> = [
    ['problems', t('Problems'), doc?.issues.length ?? 0],
    ['queue', t('Review'), queue.length],
    ['values', t('Values'), doc?.values.length ?? 0],
    ['questions', t('Open questions'), doc?.questions.filter((q) => q.state === 'open').length ?? 0],
    ['source', t('Source'), 0],
  ];

  return (
    <section className={`bottom${collapsed ? ' collapsed' : ''}`}>
      <div className="tabs">
        {counts.map(([key, label, count]) => (
          <button
            key={key}
            className={`tab${key === tab ? ' on' : ''}`}
            onClick={() => {
              setTab(key);
              if (collapsed) setCollapsed(false);
            }}
          >
            {label}
            {count > 0 && <span className="count">{count}</span>}
          </button>
        ))}
        <span className="grow" />
        <button
          className="collapse"
          title={collapsed ? t('Expand') : t('Collapse')}
          onClick={() => setCollapsed(!collapsed)}
        >
          {collapsed ? '▲' : '▼'}
        </button>
      </div>

      {!collapsed && (
        <div className="tab-body">
          {tab === 'problems' && (
            doc && doc.issues.length > 0
              ? doc.issues.map((issue, index) => (
                <div
                  key={`${issue.code}-${issue.line}-${index}`}
                  className="row-line clickable"
                  title={t('Click to select the element this is about')}
                  onClick={() => {
                    // A diagnostic names a line even when it names no element — a syntax error is
                    // the whole file's, not one box's — so the line is the fallback. Without it a
                    // click on the most common kind of problem did nothing at all.
                    if (issue.at && onLocateIfFound(issue.at)) return;
                    onLocateLine(issue.line);
                  }}
                >
                  <span className={`tag ${issue.severity === 'error' ? 'bad' : 'warn'}`}>
                    {issue.severity === 'error' ? t('error') : t('warning')}
                  </span>
                  <span className="k mono">{issue.code}</span>
                  <span className="v">{issue.message}</span>
                  <span className="k mono">{`${issue.at || '—'}:${issue.line}`}</span>
                </div>
              ))
              : <p className="note">{t('This model has no problems.')}</p>
          )}

          {tab === 'queue' && (
            doc
              ? (
                <QueuePanel
                  items={queue}
                  active={queueActive}
                  setActive={setQueueActive}
                  onLocate={onLocate}
                  onJudge={onJudge}
                  submit={submit}
                  toast={toast}
                  reviewer={reviewer}
                  setReviewer={setReviewer}
                />
              )
              : <p className="note">{t('Nothing to review.')}</p>
          )}

          {tab === 'values' && (
            doc && doc.values.length > 0
              ? doc.values.map((value) => (
                <div key={value.key} className="row-line clickable" onClick={() => onLocate(value.key)}>
                  <span className="k mono">{value.key}</span>
                  <span className="v mono">{value.text}</span>
                  <span className={`tag ${value.origin === 'assumed' ? 'warn' : value.origin ? 'ok' : 'bad'}`}>
                    {value.origin ?? t('no record')}
                  </span>
                </div>
              ))
              : <p className="note">{t('This model has no values.')}</p>
          )}

          {tab === 'questions' && (
            doc && doc.questions.length > 0
              ? doc.questions.map((question) => (
                <div
                  key={question.id}
                  className="row-line clickable"
                  onClick={() => question.at && onLocate(question.at)}
                >
                  <span className={`tag ${question.state === 'open' ? 'warn' : 'ok'}`}>{question.state}</span>
                  <span className="v">{question.question}</span>
                  <span className="k mono">{question.at}</span>
                </div>
              ))
              : <p className="note">{t('The ledger has no open questions.')}</p>
          )}

          {tab === 'source' && <SourceView elementKey={selected} />}
        </div>
      )}
    </section>
  );
}
