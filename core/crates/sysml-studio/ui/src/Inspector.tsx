import { useState } from 'react';
import type { CanvasDocument, CanvasValue } from './types';
import { waitingOf } from './waiting';
import type { Plan } from './api';

/**
 * Why an element holds the model, in words.
 *
 * The wire carries a code so the reason can be added without the canvas shipping in step; the
 * sentence is the UI's own, which is what lets it be translated like every other string here.
 */
const REASON: Record<string, string> = {
  'literal-only': 'the constraint has only constants in it',
  'unmet': 'nothing in the model carries this requirement',
  'unaccounted': 'this value has no ledger entry',
  'broken': 'one end of this connection does not exist',
  'undecided': 'this requirement has never been judged',
  'uncovered-concern': 'no requirement is derived from this concern',
};
import { IssueBox, KV, Section } from './ui';
import type { AlignMode } from './align';
import { t } from './i18n';

export interface Submit {
  (path: string, body: Record<string, unknown>): Promise<{ ok?: boolean; error?: string }>;
}

/**
 * Nothing selected: what is waiting for the person.
 *
 * The bottom panel shows the problems, and the status bar shows how big the model is, so neither
 * is repeated here. What is left is the set of *outstanding* facts — things a person has to
 * resolve — because those are the ones worth a glance before clicking anything.
 *
 * A number that is zero is not shown. A panel of eight boxes reading `0` teaches the reader to
 * skip the panel, and the one that is not zero is then missed.
 */
function Overview({ doc }: { doc: CanvasDocument }) {
  const waiting = waitingOf(doc.summary);
  if (waiting.length === 0) {
    return (
      <p className="idle-clear">
        {t('Nothing is waiting: every value has a record, every assumption is approved, no question is open.')}
      </p>
    );
  }
  return (
    <div className="overview">
      {waiting.map((entry) => (
        <div key={entry.key} className="stat warn">
          <div className="n">{entry.count}</div>
          <div className="k">{t(entry.key)}</div>
        </div>
      ))}
    </div>
  );
}

/**
 * A text field that commits on Enter and on blur, and puts the old text back when the server
 * refuses.
 *
 * Enter is handled directly rather than relying on the blur it would cause: a synthetic Enter (an
 * automation harness, an assistive tool) need not produce a focus change, and an edit that
 * silently does nothing is the worst failure this panel can have.
 */
function CommitField({
  label,
  initial,
  onCommit,
  placeholder,
  select,
}: {
  label: string;
  initial: string;
  onCommit: (next: string) => Promise<void>;
  placeholder?: string;
  select?: () => string;
}) {
  const [text, setText] = useState(initial);
  const [committing, setCommitting] = useState(false);

  const commit = async () => {
    if (committing) return;
    const next = text.trim();
    const current = select ? select() : initial;
    if (!next || next === current) return;
    setCommitting(true);
    try {
      await onCommit(next);
    } finally {
      setCommitting(false);
    }
  };

  return (
    <div className="field">
      <label>{label}</label>
      <input
        type="text"
        value={text}
        spellCheck={false}
        placeholder={placeholder}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') { event.preventDefault(); void commit(); }
          if (event.key === 'Escape') { setText(initial); event.currentTarget.blur(); }
        }}
        onBlur={() => void commit()}
      />
    </div>
  );
}

function ValueSection({
  value,
  submit,
  toast,
}: {
  value: CanvasValue;
  submit: Submit;
  toast: (message: string, bad?: boolean) => void;
}) {
  const [approver, setApprover] = useState('');
  const [origin, setOrigin] = useState('spec');
  const [why, setWhy] = useState('');
  const [falsifier, setFalsifier] = useState('');
  const [cite, setCite] = useState('');

  return (
    <Section title={t('Value and provenance')}>
      {/* The value comes before the name: a feature is opened to change its number far more
          often than to rename it, and putting the rename field first makes the number look
          secondary. */}
      <CommitField
        key={`${value.key}:${value.text}`}
        label={t('Current value')}
        initial={value.text}
        onCommit={async (next) => {
          const result = await submit('/api/set-value', { key: value.key, value: next });
          if (result.ok === false) toast(result.error ?? t('The write was refused'), true);
        }}
      />

      {/* What the expression comes to. `Area(2.0, 3.0)` and `6` are one number, and the field
          above shows the first; without this the panel reads as a page of formulas with no
          answers, which is what a parametric model looked like before the evaluator ran calls. */}
      {value.computed && (
        <div className="line">
          <span>{t('Comes to')}</span>
          <span className="v mono">{value.computed}</span>
        </div>
      )}

      {/* What kind of quantity the number is. Three states, and the third is the one that
          matters: a unit outside the table leaves every comparison involving this value
          undecided, which is not the same as a bare number that compares with anything. */}
      {value.unit !== '' && (
        value.unit_known ? (
          <div className="line">
            <span>{t('Quantity')}</span>
            <span className="v mono">{`${value.unit} · ${value.dimension}`}</span>
          </div>
        ) : (
          <div className="line">
            <span>{t('Quantity')}</span>
            <span className="v mono">{t('{unit} is not in the unit table; comparisons stay undecided', { unit: value.unit })}</span>
          </div>
        )
      )}

      {value.origin ? (
        <>
          <div className="line">
            <span className={`tag ${value.origin === 'assumed' ? 'warn' : 'ok'}`}>
              {`${value.origin} · ${value.decision_id}`}
            </span>
            <span className="v">
              {value.approved_by
                ? value.approved_by.replace('human:', '')
                : (value.origin === 'assumed' ? t('unapproved') : '')}
            </span>
          </div>
          {value.why && <KV name={t('basis')} value={value.why} />}
          {value.falsifier && <KV name={t('falsifier')} value={value.falsifier} />}
          {value.cites.length > 0 && <KV name={t('cited sources')} value={value.cites.join(', ')} />}
          {value.origin === 'assumed' && !value.approved_by && (
            <div className="field">
              <label>{t('Approve (only a person can approve an assumption)')}</label>
              <div className="row">
                <input
                  type="text"
                  placeholder={t('your name')}
                  value={approver}
                  onChange={(event) => setApprover(event.target.value)}
                />
                <button
                  className="act primary"
                  onClick={async () => {
                    const name = approver.trim();
                    if (!name) { toast(t('Fill in a name'), true); return; }
                    const result = await submit('/api/approve', { id: value.decision_id, by: `human:${name}` });
                    if (result.ok === false) toast(result.error ?? t('Approval failed'), true);
                  }}
                >
                  {t('Approve')}
                </button>
              </div>
            </div>
          )}
        </>
      ) : (
        <>
          <p className="note">
            {t('No ledger entry records this number. Say where it came from and the gate will pass it.')}
          </p>
          <div className="field">
            <label>{t('Record a source')}</label>
            {/* The origin names stay as the ledger writes them: `spec`, `derived`, `assumed`
                are what `prov` prints and what a decision record stores. The text after each is
                the explanation, not a translation of the keyword. */}
            <select value={origin} onChange={(event) => setOrigin(event.target.value)}>
              <option value="spec">{t('spec — the document states this value')}</option>
              <option value="derived">{t('derived — computed from other values')}</option>
              <option value="assumed">{t('assumed — my judgement')}</option>
            </select>
            <input
              type="text"
              placeholder={t('basis, one sentence')}
              value={why}
              style={{ marginTop: 6 }}
              onChange={(event) => setWhy(event.target.value)}
            />
            <input
              type="text"
              placeholder={t('falsifier: what observation would show it is wrong')}
              value={falsifier}
              style={{ marginTop: 6 }}
              onChange={(event) => setFalsifier(event.target.value)}
            />
            <input
              type="text"
              placeholder={t('source id (spec requires one, e.g. S001)')}
              value={cite}
              style={{ marginTop: 6 }}
              onChange={(event) => setCite(event.target.value)}
            />
            <button
              className="act"
              style={{ marginTop: 6 }}
              onClick={async () => {
                const result = await submit('/api/decide', {
                  key: value.key,
                  value: value.text,
                  origin,
                  why,
                  falsifier,
                  cite: cite || undefined,
                  by: 'human:canvas',
                });
                if (result.ok === false) toast(result.error ?? t('Recording failed'), true);
              }}
            >
              {t('Record')}
            </button>
          </div>
        </>
      )}
    </Section>
  );
}

const ALIGN_BUTTONS: Array<[AlignMode, string]> = [
  ['left', 'Align left'],
  ['center-h', 'Centre horizontally'],
  ['right', 'Align right'],
  ['top', 'Align top'],
  ['center-v', 'Centre vertically'],
  ['bottom', 'Align bottom'],
  ['distribute-h', 'Distribute horizontally'],
  ['distribute-v', 'Distribute vertically'],
];

/**
 * More than one box selected: what can be done to the group.
 *
 * Alignment is a layout change, not a model change — it moves boxes in the sidecar and says
 * nothing about the text. That is why it lives here rather than on the element panel: with one box
 * selected there is nothing to align it to.
 */
function AlignPanel({
  selection,
  onAlign,
}: {
  selection: string[];
  onAlign: (how: AlignMode) => void;
}) {
  return (
    <aside className="inspector">
      <h2 className="sheet">{t('{n} selected', { n: selection.length })}</h2>
      <p className="sheet-meta">{t('Ctrl-click a box to add it to the selection.')}</p>
      <Section title={t('Align')}>
        <div className="align-grid">
          {ALIGN_BUTTONS.map(([how, label]) => (
            <button key={how} className="act" title={t(label)} onClick={() => onAlign(how)}>
              {t(label)}
            </button>
          ))}
        </div>
        <p className="note">{t('Moves the boxes. The model text is not touched.')}</p>
      </Section>
    </aside>
  );
}

export function Inspector({
  doc,
  plan,
  selection,
  select,
  onAlign,
  submit,
  toast,
}: {
  doc: CanvasDocument | null;
  /** The ladder, for the one question the picture cannot answer: is this box in the way? */
  plan: Plan | null;
  selection: string[];
  select: (keys: string[]) => void;
  onAlign: (how: AlignMode) => void;
  submit: Submit;
  toast: (message: string, bad?: boolean) => void;
}) {
  const [addKind, setAddKind] = useState('part');
  const [addName, setAddName] = useState('');
  const selected = selection.length > 0 ? selection[selection.length - 1] : null;
  const node = doc?.nodes.find((n) => n.key === selected) ?? null;

  if (!doc) return <aside className="inspector" />;
  if (selection.length > 1) return <AlignPanel selection={selection} onAlign={onAlign} />;
  if (!node) {
    return (
      <aside className="inspector">
        <div className="inspector-idle">
          <p className="idle-title">{t('Nothing selected')}</p>
          <p className="idle-note">{t('Click a box to see its properties, provenance and problems.')}</p>
        </div>
        <Overview doc={doc} />
      </aside>
    );
  }

  const value = doc.values.find((v) => v.key === node.key) ?? null;
  const judgement = doc.judgements.find((j) => j.key === node.key) ?? null;
  const issues = doc.issues.filter((i) => i.at && node.key.endsWith(i.at));
  const related = doc.edges.filter((e) => e.from_key === node.key || e.to_key === node.key);
  const byKey = new Map(doc.nodes.map((n) => [n.key, n]));

  return (
    <aside className="inspector">
      <h2 className="sheet">{node.label}</h2>
      <p className="sheet-meta">{`«${node.stereotype}»${node.is_def ? ' def' : ''}`}</p>

      {/* The one question a picture cannot answer: is this box in the way?
          Silence when it is not — an inspector that says "fine" about every box teaches people to
          skip the line that matters. */}
      {plan && plan.blocking[node.key] && (
        <Section title={t('Blocking stage {n}', { n: plan.number })}>
          <div className="line">
            <span className="tag bad">{t(REASON[plan.blocking[node.key]] ?? plan.blocking[node.key])}</span>
          </div>
        </Section>
      )}

      {judgement && (
        <Section title={t('Verdict')}>
          <div className="line">
            <span className={`tag ${judgement.stale ? 'warn' : judgement.verdict === 'pass' ? 'ok' : 'bad'}`}>
              {judgement.verdict}
            </span>
            <span className="v">{judgement.stale ? t('stale verdict') : judgement.at}</span>
          </div>
          {judgement.detail && <KV name={t('detail')} value={judgement.detail} />}
          <KV name={t('judged by')} value={judgement.judged_by || t('unknown')} />
          {judgement.stale && (
            <p className="note">
              {t('This verdict was computed against an older model. Re-judge it before relying on it.')}
            </p>
          )}
        </Section>
      )}

      {value && <ValueSection key={value.key} value={value} submit={submit} toast={toast} />}

      <Section title={t('Identity')}>
        {/* `qualified name` keeps the term the rest of the toolchain prints in `resolve` and
            `check`; it is not renamed here. */}
        <KV name="qualified name" value={node.key} mono />
        {node.placed && (
          <div className="row">
            <button
              className="act"
              onClick={async () => {
                const result = await submit('/api/reset-placement', { key: node.key });
                if (result.ok === false) toast(result.error ?? t('Reset failed'), true);
              }}
            >
              {t('Reset to automatic position')}
            </button>
          </div>
        )}
        <CommitField
          key={`${node.key}:${node.label}`}
          label={t('Rename')}
          initial={node.label.split(' : ')[0]}
          onCommit={async (next) => {
            const result = await submit('/api/rename', { key: node.key, name: next });
            if (result.ok === false) toast(result.error ?? t('Rename failed'), true);
          }}
        />
      </Section>

      {node.rows.length > 0 && (
        <Section title={t('Members')}>
          {node.rows.map((row, i) => <KV key={i} name="" value={row} />)}
        </Section>
      )}

      {issues.length > 0 && (
        <Section title={t('Problems {n}', { n: issues.length })}>
          {issues.map((issue, i) => <IssueBox key={i} issue={issue} />)}
        </Section>
      )}

      {related.length > 0 && (
        <Section title={t('Relationships {n}', { n: related.length })}>
          {related.map((edge) => {
            const outgoing = edge.from_key === node.key;
            const other = byKey.get(outgoing ? edge.to_key : edge.from_key);
            return (
              <div
                key={edge.index}
                className="line"
                style={{ cursor: other ? 'pointer' : 'default' }}
                onClick={() => other && select([other.key])}
              >
                <span>{outgoing ? `${edge.arrow} →` : `← ${edge.arrow}`}</span>
                <span className="v mono">{other ? other.label : t('(unresolved)')}</span>
              </div>
            );
          })}
        </Section>
      )}

      <Section title={t('Add a member')}>
        <div className="row">
          <select value={addKind} onChange={(event) => setAddKind(event.target.value)}>
            {['part', 'attribute', 'port', 'requirement'].map((k) => (
              <option key={k} value={k}>{k}</option>
            ))}
          </select>
          <input
            type="text"
            placeholder={t('name')}
            spellCheck={false}
            value={addName}
            onChange={(event) => setAddName(event.target.value)}
          />
          <button
            className="act"
            onClick={async () => {
              const name = addName.trim();
              if (!name) { toast(t('Fill in a name'), true); return; }
              const result = await submit('/api/add-member', { owner: node.key, kind: addKind, name });
              if (result.ok === false) { toast(result.error ?? t('Adding failed'), true); return; }
              setAddName('');
            }}
          >
            {t('Add')}
          </button>
        </div>
        <p className="note">
          {t('Writes into the model text and re-parses immediately. Nothing is written if it does not parse.')}
        </p>
      </Section>
    </aside>
  );
}
