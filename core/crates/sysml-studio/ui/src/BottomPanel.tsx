import { Tabs, Badge, Button, Select } from '@radix-ui/themes';
import type { CanvasDocument } from './types';
import { SourceView } from './SourceView';
import { useState } from 'react';
import { t } from './i18n';
import { codeCounts, filterProblems, NO_FILTER, severityCounts, type ProblemFilter } from './problems';
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

  const [problemFilter, setProblemFilter] = useState<ProblemFilter>(NO_FILTER);
  const shownProblems = doc ? filterProblems(doc.issues, problemFilter) : [];

  return (
    <section className={`bottom${collapsed ? ' collapsed' : ''}`}>
      <Tabs.Root
        value={tab}
        onValueChange={(next) => {
          setTab(next as BottomTab);
          if (collapsed) setCollapsed(false);
        }}
      >
        <Tabs.List size="1">
          {counts.map(([key, label, count]) => (
            <Tabs.Trigger key={key} value={key}>
              {label}
              {count > 0 && <Badge size="1" variant="soft" ml="1">{count}</Badge>}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
      </Tabs.Root>
      <button
        className="collapse"
        title={collapsed ? t('Expand') : t('Collapse')}
        onClick={() => setCollapsed(!collapsed)}
      >
        {collapsed ? '▲' : '▼'}
      </button>

      {!collapsed && (
        <div className="tab-body">
          {tab === 'problems' && (
            doc && doc.issues.length > 0
              ? (
                <>
                  <div className="problem-filter">
                    {(['all', 'error', 'warning', 'info'] as const).map((severity) => {
                      const counts = severityCounts(doc.issues);
                      const label =
                        severity === 'all' ? t('all') : severity === 'error' ? t('errors') : severity === 'warning' ? t('warnings') : t('info');
                      return (
                        <Button
                          key={severity}
                          size="1"
                          variant={problemFilter.severity === severity ? 'solid' : 'ghost'}
                          onClick={() => setProblemFilter({ ...problemFilter, severity })}
                        >
                          {`${label} ${counts[severity]}`}
                        </Button>
                      );
                    })}
                    <Select.Root
                      value={problemFilter.code ?? 'all'}
                      onValueChange={(value) =>
                        setProblemFilter({ ...problemFilter, code: value === 'all' ? null : value })
                      }
                    >
                      <Select.Trigger variant="soft" placeholder={t('every rule')} />
                      <Select.Content>
                        <Select.Item value="all">{t('every rule')}</Select.Item>
                        {codeCounts(doc.issues).map(({ code, count }) => (
                          <Select.Item key={code} value={code}>
                            {`${code} (${count})`}
                          </Select.Item>
                        ))}
                      </Select.Content>
                    </Select.Root>
                  </div>
                  {shownProblems.length === 0 && (
                    <p className="note">{t('No problem matches this filter.')}</p>
                  )}
                  {shownProblems.map((issue, index) => (
                    <div
                      key={`${issue.code}-${issue.line}-${index}`}
                      className="row-line clickable"
                      title={t('Click to select the element this is about')}
                      onClick={() => {
                        // A diagnostic names a line even when it names no element — a syntax error
                        // is the whole file's, not one box's — so the line is the fallback. Without
                        // it a click on the most common kind of problem did nothing at all.
                        if (issue.at && onLocateIfFound(issue.at)) return;
                        onLocateLine(issue.line);
                      }}
                    >
                      <Badge size="1" variant="soft" color={issue.severity === 'error' ? 'red' : issue.severity === 'warning' ? 'amber' : 'gray'}>
                        {t(issue.severity)}
                      </Badge>
                      <span className="k mono">{issue.code}</span>
                      <span className="v">{issue.message}</span>
                      <span className="k mono">{`${issue.at || '—'}:${issue.line}`}</span>
                    </div>
                  ))}
                </>
              )
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
                  <Badge size="1" variant="soft" color={value.origin === 'assumed' ? 'amber' : value.origin ? 'green' : 'red'}>
                    {value.origin ?? t('no record')}
                  </Badge>
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
                  <Badge size="1" variant="soft" color={question.state === 'open' ? 'amber' : 'green'}>{question.state}</Badge>
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
