// The document the server sends. Field names match `sysml-studio/src/document.rs` one for one:
// the studio renders this, it does not reinterpret it, and a rename on either side should be a
// compile error rather than a blank panel.

export type Point = [number, number];

export interface CanvasPort {
  name: string;
  direction: string | null;
  side: 'left' | 'right';
}

export interface CanvasNode {
  key: string;
  index: number;
  stereotype: string;
  label: string;
  is_def: boolean;
  is_frame: boolean;
  unmet: boolean;
  x: number;
  y: number;
  w: number;
  h: number;
  placed: boolean;
  rows: string[];
  ports: CanvasPort[];
  parent: number | null;
}

export interface CanvasFrame {
  key: string;
  stereotype: string;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  title: number;
}

export type ArrowKind = 'composite' | 'typing' | 'connector' | 'satisfy' | 'annotate';
// `annotate` is an edge the SVG export has drawn since annotations became edges; the canvas
// has to know it too, or the two pictures disagree about the same model.

export interface CanvasEdge {
  index: number;
  from: number;
  to: number;
  arrow: ArrowKind;
  label: string | null;
  points: Point[];
  hops: Point[];
  from_key: string;
  to_key: string;
}

export interface CanvasIssue {
  severity: 'error' | 'warning';
  code: string;
  message: string;
  at: string;
  line: number;
}

export interface CanvasValue {
  key: string;
  text: string;
  line: number;
  origin: string | null;
  why: string | null;
  falsifier: string | null;
  approved_by: string | null;
  cites: string[];
  decision_id: string | null;
  /** The unit as written, empty when the value states none. */
  unit: string;
  /** `mass`, `length/time`, … when the unit table knows the unit; empty otherwise. */
  dimension: string;
  /** False for a unit outside the table: comparisons involving it stay undecided. */
  unit_known: boolean;
  /** What the expression comes to, when it comes to anything. */
  computed?: string | null;
}

export interface CanvasQuestion {
  id: string;
  question: string;
  at: string;
  blocks: string[];
  state: 'open' | 'answered' | 'deferred';
}

/**
 * One recorded verdict.
 *
 * The verdict is about the requirement *and the numbers it read*, so the record carries the
 * fingerprint it was judged against. `stale` says the model has moved since; the badge must then
 * stop reading as a current answer, or it would claim something the record does not support.
 */
export interface CanvasJudgement {
  key: string;
  verdict: 'pass' | 'fail' | 'inconclusive' | 'error';
  detail: string;
  judged_by: string;
  at: string;
  judged_against: string;
  stale: boolean;
}

export interface Summary {
  nodes: number;
  edges: number;
  errors: number;
  warnings: number;
  values_without_record: number;
  assumptions_unapproved: number;
  open_questions: number;
  judgements: number;
  judgements_stale: number;
}

export interface CanvasDocument {
  title: string;
  files: string[];
  fingerprint: string;
  view: string;
  mode: string;
  width: number;
  height: number;
  nodes: CanvasNode[];
  frames: CanvasFrame[];
  edges: CanvasEdge[];
  issues: CanvasIssue[];
  values: CanvasValue[];
  questions: CanvasQuestion[];
  /** The ledger fingerprint a verdict is judged against. Not `fingerprint`, which digests the
   * whole text: a comment moves that one and must not invalidate a verdict. */
  ledger_fingerprint: string;
  judgements: CanvasJudgement[];
  summary: Summary;
}

export interface SourceRange {
  ok: boolean;
  error?: string;
  text: string;
  path: string;
  line: number;
  endLine: number;
  nameStart: number;
  nameEnd: number;
}

export type ViewKey = 'gv' | 'iv' | 'tv' | 'pv' | 'stv' | 'afv' | 'sv';
export type ModeKey = 'extended' | 'compat';

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Camera {
  x: number;
  y: number;
  zoom: number;
}

/**
 * Which compartments to draw.
 *
 * A box in a real model carries a stereotype, rows and ports at once, and which of those a reader
 * wants depends on what they are checking: the stereotype while triaging kinds, the rows while
 * comparing values, the ports while following connectors. Hiding one keeps the box's size, so the
 * picture does not reflow under a click.
 */
export interface Display {
  stereotype: boolean;
  members: boolean;
  ports: boolean;
}

/** One view the model declares (`view states : StateTransitionView;`). */
export interface ViewEntry {
  key: string;
  name: string;
  target: string;
  /** The projection that type asks for, or empty when nothing here draws it. */
  projection: string;
}
