import { useEffect, useRef, useState } from 'react';
import type { QueueItem, QueueKind } from './queue';
import type { Submit } from './Inspector';
import { t } from './i18n';

const KIND_LABEL: Record<QueueKind, string> = {
  unrecorded: 'no record',
  unapproved: 'unapproved',
  'stale-verdict': 'stale',
  problem: 'problem',
};

const KIND_CLASS: Record<QueueKind, string> = {
  unrecorded: 'bad',
  unapproved: 'warn',
  'stale-verdict': 'warn',
  problem: '',
};

/**
 * The review queue.
 *
 * The row is the unit of work, not the diagram: a reviewer clears a list. Each row carries the
 * action that clears it, so acting never means leaving the queue to go find a form — that is the
 * difference between a report and a queue.
 *
 * Keyboard: `j` / `k` (or the arrows) move, Enter acts on the active row, Escape drops it. The
 * active row's action is the one an Enter runs, which is why a row with nothing to do but look at
 * it says so instead of pretending.
 */
export function QueuePanel({
  items,
  active,
  setActive,
  onLocate,
  onJudge,
  submit,
  toast,
  reviewer,
  setReviewer,
}: {
  items: QueueItem[];
  active: string | null;
  setActive: (id: string | null) => void;
  onLocate: (key: string) => void;
  onJudge: (requirement: string) => Promise<void>;
  submit: Submit;
  toast: (message: string, bad?: boolean) => void;
  reviewer: string;
  setReviewer: (name: string) => void;
}) {
  const [origin, setOrigin] = useState('assumed');
  const [why, setWhy] = useState('');
  const [cite, setCite] = useState('');
  const [name, setName] = useState(reviewer);
  const whyField = useRef<HTMLInputElement>(null);
  const nameField = useRef<HTMLInputElement>(null);
  const body = useRef<HTMLDivElement>(null);

  const current = items.find((item) => item.id === active) ?? null;

  // A form belongs to one row. Carrying half-typed text to the next row would post a reason that
  // was written about something else.
  useEffect(() => {
    setWhy('');
    setCite('');
    setOrigin('assumed');
  }, [active]);

  const record = async (item: QueueItem) => {
    const result = await submit('/api/decide', {
      key: item.key,
      value: item.detail,
      origin,
      why: why.trim(),
      falsifier: '',
      cite: cite.trim() || undefined,
      by: `human:${reviewer || 'canvas'}`,
    });
    if (result.ok === false) toast(result.error ?? t('Recording failed'), true);
  };

  const approve = async (item: QueueItem) => {
    if (!item.decisionId) return;
    const who = (name || reviewer).trim();
    if (!who) { nameField.current?.focus(); toast(t('Fill in a name'), true); return; }
    setReviewer(who);
    const result = await submit('/api/approve', {
      id: item.decisionId,
      by: `human:${who}`,
    });
    if (result.ok === false) toast(result.error ?? t('Approval failed'), true);
  };

  /** What Enter does on this row. */
  const primary = (item: QueueItem) => {
    if (item.kind === 'unrecorded') { whyField.current?.focus(); return; }
    if (item.kind === 'unapproved') { void approve(item); return; }
    if (item.kind === 'stale-verdict' && item.requirement) { void onJudge(item.requirement); return; }
    onLocate(item.key);
  };

  const move = (delta: number) => {
    if (items.length === 0) return;
    const index = items.findIndex((item) => item.id === active);
    const next = index < 0
      ? (delta > 0 ? 0 : items.length - 1)
      : Math.min(items.length - 1, Math.max(0, index + delta));
    setActive(items[next].id);
    onLocate(items[next].key);
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    // A field inside the row owns its keys; the queue only listens when nothing is being typed.
    const typing = (event.target as HTMLElement).tagName === 'INPUT';
    if (typing && event.key !== 'Escape') return;
    if (event.key === 'j' || event.key === 'ArrowDown') { event.preventDefault(); move(1); }
    else if (event.key === 'k' || event.key === 'ArrowUp') { event.preventDefault(); move(-1); }
    else if (event.key === 'Enter') {
      if (typing) return;
      if (current) { event.preventDefault(); primary(current); }
    } else if (event.key === 'Escape') setActive(null);
  };

  if (items.length === 0) {
    return <p className="note">{t('Nothing to review.')}</p>;
  }

  return (
    <div className="queue" ref={body} tabIndex={0} onKeyDown={onKeyDown}>
      {items.map((item) => {
        const isActive = item.id === active;
        return (
          <div
            key={item.id}
            className={`qrow${isActive ? ' on' : ''}`}
            onClick={() => { setActive(item.id); onLocate(item.key); }}
          >
            <span className={`tag ${KIND_CLASS[item.kind]}`}>{t(KIND_LABEL[item.kind])}</span>
            <span className="qtitle">{item.title}</span>
            <span className="qdetail">{item.detail}</span>

            {isActive && item.kind === 'unrecorded' && (
              <div className="qact" onClick={(event) => event.stopPropagation()}>
                <select value={origin} onChange={(event) => setOrigin(event.target.value)}>
                  <option value="spec">{t('spec — the document states this value')}</option>
                  <option value="derived">{t('derived — computed from other values')}</option>
                  <option value="assumed">{t('assumed — my judgement')}</option>
                </select>
                <input
                  ref={whyField}
                  type="text"
                  placeholder={t('basis, one sentence')}
                  value={why}
                  onChange={(event) => setWhy(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') { event.preventDefault(); void record(item); }
                  }}
                />
                <input
                  type="text"
                  placeholder={t('source id (spec requires one, e.g. S001)')}
                  value={cite}
                  onChange={(event) => setCite(event.target.value)}
                />
                <button className="act primary" onClick={() => void record(item)}>{t('Record')}</button>
              </div>
            )}

            {isActive && item.kind === 'unapproved' && (
              <div className="qact" onClick={(event) => event.stopPropagation()}>
                <input
                  ref={nameField}
                  type="text"
                  placeholder={t('your name')}
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
                <button className="act primary" onClick={() => void approve(item)}>{t('Approve')}</button>
              </div>
            )}

            {isActive && item.kind === 'stale-verdict' && (
              <div className="qact" onClick={(event) => event.stopPropagation()}>
                <button
                  className="act primary"
                  onClick={() => item.requirement && void onJudge(item.requirement)}
                >
                  {t('Re-judge')}
                </button>
              </div>
            )}
          </div>
        );
      })}
      <p className="note">{t('j / k moves, Enter acts on the selected row.')}</p>
    </div>
  );
}
