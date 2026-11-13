// Renders a sample model through the same tokenizer and the same classes the app uses, so the
// colouring can be looked at without driving the whole studio.
//
// This is a harness, not the app: it proves what the tokens look like, and the app is the same
// `rows()` call inside `<pre className="source">`.

import { writeFileSync } from 'node:fs';
import { rows } from './src/highlight.ts';

const SAMPLE = `package Brake {
    private import ScalarValues::*;
    private import ISQ::*;

    // A pad is rated, not measured.
    metadata def Safety {
        attribute level : Integer;
        attribute rationale : String;
    }

    calc def Power {
        in force : ForceValue;
        in speed : SpeedValue;
        return p : PowerValue = force * speed;
    }

    @Safety { level = 3; rationale = "hazard analysis"; }
    part def Brake {
        attribute f : ForceValue = 10.0 [N];
        attribute v : SpeedValue = 2.0 [m/s];
        attribute p : PowerValue = Power(f, v);

        /* A block comment
           spanning two lines. */
        constraint budget {
            assert p <= 25.0 [W];
        }
    }
}
`;

const escape = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

const body = rows(SAMPLE)
  .map((row, index) => {
    const cells = row
      .map((token) => (token.kind === 'plain'
        ? escape(token.text)
        : `<span class="tok ${token.kind}">${escape(token.text)}</span>`))
      .join('');
    return `<span class="srcline${index >= 18 && index < 26 ? ' on' : ''}">${cells}\n</span>`;
  })
  .join('');

const html = `<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>source colouring</title>
<style>
  body { margin: 0; padding: 16px; background: #fbfbfa; color: #1c1c1a;
         font: 13px/1.55 -apple-system, "Segoe UI", "Noto Sans SC", sans-serif; }
  pre.source { margin: 0; padding: 8px 9px; background: #f1f1ed; border-radius: 4px;
               font-family: ui-monospace, Consolas, monospace; font-size: 11.5px; line-height: 1.5; }
  .srcline.on { background: #fffbe6; box-shadow: inset 2px 0 0 #c9a227; }
  .tok.comment { color: #9b9b95; font-style: italic; }
  .tok.string { color: #2f6b4f; }
  .tok.number { color: #8a6100; }
  .tok.keyword { color: #3b5b8a; }
  .tok.annotation { color: #6b4a8a; }
</style></head>
<body><pre class="source">${body}</pre></body></html>`;

writeFileSync(process.argv[2], html, 'utf8');
console.log(`wrote ${process.argv[2]}`);
