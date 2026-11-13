import type { CanvasIssue } from './types';

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="section">
      <h3>{title}</h3>
      {children}
    </div>
  );
}

export function KV({ name, value, mono }: { name: string; value: string; mono?: boolean }) {
  return (
    <div className="line">
      {name && <span>{name}</span>}
      <span className={`v${mono ? ' mono' : ''}`}>{value}</span>
    </div>
  );
}

export function IssueBox({ issue }: { issue: CanvasIssue }) {
  return (
    <div className={`issue ${issue.severity === 'error' ? 'bad' : 'warn'}`}>
      <span className="code">{issue.code}</span>
      {issue.message}
    </div>
  );
}
