import { useEffect, useRef, useState } from 'react';
import type { SourceRange } from './types';
import { fetchSource } from './api';
import { rows } from './highlight';
import { t } from './i18n';

/**
 * The element's own lines, in its own file.
 *
 * Shows the whole buffer with the declaration marked, not a fixed window: a container owns the
 * lines its members occupy and an `attribute` occupies one, so any constant window is wrong in one
 * direction or the other.
 */
export function SourceView({ elementKey }: { elementKey: string | null }) {
  const [data, setData] = useState<SourceRange | null>(null);
  const pre = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (!elementKey) {
      setData(null);
      return;
    }
    let live = true;
    setData(null);
    fetchSource(elementKey).then((result) => { if (live) setData(result); });
    return () => { live = false; };
  }, [elementKey]);

  useEffect(() => {
    const node = pre.current;
    if (!node || !data?.ok) return;
    // Scroll by line arithmetic, not by measuring a span. `offsetTop` is relative to the span's
    // offset parent, which is not necessarily the `pre`, so a value measured from a higher
    // ancestor would scroll the declaration off the top. Every line box here is the same height,
    // so the position is `line * lineHeight`.
    const lineHeight = parseFloat(getComputedStyle(node).lineHeight) || 16.5;
    const from = Math.max(0, data.line - 1);
    node.scrollTop = Math.max(0, from * lineHeight - node.clientHeight / 3);
  }, [data]);

  if (!elementKey) {
    return <p className="note">{t('Select an element: click a box on the canvas or an entry in the navigator.')}</p>;
  }

  const grid = data?.ok ? rows(data.text) : [];
  const from = data?.ok ? Math.max(0, data.line - 1) : 0;
  const to = data?.ok ? Math.min(grid.length, data.endLine) : 0;

  return (
    <div className="source-wrap">
      {data === null && <pre className="source">{t('Reading…')}</pre>}
      {data !== null && !data.ok && <pre className="source">{data.error ?? t('Could not read the file')}</pre>}
      {data !== null && data.ok && (
        <>
          <pre className="source tall" ref={pre}>
            {/* Colour is per token and the plain pieces are emitted as bare text: a span around
                every punctuation mark would triple the nodes of a file for no gain. */}
            {grid.map((row, i) => (
              <span key={i} className={i >= from && i < to ? 'srcline on' : 'srcline'}>
                {row.map((token, j) => (
                  token.kind === 'plain'
                    ? token.text
                    : <span key={j} className={`tok ${token.kind}`}>{token.text}</span>
                ))}
                {'\n'}
              </span>
            ))}
          </pre>
          {data.path && <p className="note">{data.path}</p>}
        </>
      )}
    </div>
  );
}
