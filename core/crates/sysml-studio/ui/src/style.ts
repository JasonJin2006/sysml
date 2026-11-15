/**
 * What colour a box is drawn in.
 *
 * Mirrors `sysml_view::style` in the kernel. Two pictures of one model — the PNG in a report and
 * the box on the canvas — have to agree about what a `part def` looks like, so this table is the
 * same table. A colour added there is added here, and `style.test.ts` fails if the two drift.
 *
 * Every colour encodes the kind of element the box is. A diagram is scanned by colour before any
 * label is read — that is what makes a two-hundred-element picture legible — so a colour that
 * encodes nothing is not added.
 */

export interface BoxStyle {
  /** The band behind the stereotype and name. */
  header: string;
  /** The body, where attributes and other compartments are drawn. */
  body: string;
  /** The border, and the rule between the title and the body. */
  border: string;
}

/** An outside-the-table kind: a name and a border, nothing more. */
export const PLAIN: BoxStyle = {
  header: 'var(--ui-node-header)',
  body: 'var(--ui-node-fill)',
  border: 'var(--ui-border)',
};

/** The palette file: two sets of the same table, one per interface theme. */
type Palette = Record<string, BoxStyle>;

/**
 * The exported picture's colours, which are drawn on white.
 *
 * These are `sysml_view::style`'s values verbatim. An exported SVG is a document — white page,
 * printed or opened in a viewer — so the pale bodies and the mid-tone bands are what it needs.
 */
const LIGHT: Palette = {
  'part def': { header: '#88B7E3', body: '#F7F9FF', border: '#5D9DDB' },
  part: { header: '#BDDCFF', body: '#FFFFFF', border: '#5D9DDB' },
  'enum def': { header: '#FFE066', body: '#FFFDF0', border: '#F5C518' },
  enum: { header: '#FFE066', body: '#FFFDF0', border: '#F5C518' },
  'requirement def': { header: '#F5B272', body: '#FFF5EB', border: '#E08A3C' },
  requirement: { header: '#F5B272', body: '#FFF5EB', border: '#E08A3C' },
  'constraint def': { header: '#F4969D', body: '#FFF7F9', border: '#E06874' },
  state: { header: '#80D6D6', body: '#F4FCFC', border: '#38C4C4' },
  action: { header: '#80D6A7', body: '#F6FCF8', border: '#43C479' },
  metadata: { header: '#FFFDE7', body: '#FFFDE7', border: '#C9A227' },
  comment: { header: '#FFF4E7', body: '#FFF4E7', border: '#C08A4A' },
  package: { header: '#E8EAF6', body: '#F9FAFF', border: '#C5CAE9' },
  item: { header: '#F3CC7A', body: '#FDFAEE', border: '#F0AA30' },
};

/**
 * The canvas's colours, which are drawn on a dark page.
 *
 * The same hues, moved down: a body that is nearly white on a dark canvas is a hole, not a box,
 * and the band behind a name has to stay readable against the canvas rather than against a page.
 * Each body is the export's band darkened, and each band is the export's border lightened — so a
 * reader comparing the canvas with the report still sees one family of colours per kind.
 */
const DARK: Palette = {
  'part def': { header: '#2B4A6B', body: '#1C2733', border: '#5D9DDB' },
  part: { header: '#24405C', body: '#1A2330', border: '#5D9DDB' },
  'enum def': { header: '#5A4A16', body: '#25231A', border: '#E0BC30' },
  enum: { header: '#5A4A16', body: '#25231A', border: '#E0BC30' },
  'requirement def': { header: '#6B4520', body: '#2A2119', border: '#E08A3C' },
  requirement: { header: '#6B4520', body: '#2A2119', border: '#E08A3C' },
  'constraint def': { header: '#6B2F36', body: '#2A1D1F', border: '#E06874' },
  state: { header: '#1F4A4A', body: '#182424', border: '#4FB8B8' },
  action: { header: '#1F4A34', body: '#18241D', border: '#4FB87E' },
  metadata: { header: '#3D3A20', body: '#232219', border: '#C9A227' },
  comment: { header: '#4A3524', body: '#241E19', border: '#B0895C' },
  package: { header: '#2A2E44', body: '#1D1F2B', border: '#6A6FA0' },
  item: { header: '#54431F', body: '#262117', border: '#D9A63A' },
};

/** Which palette the canvas is drawing with. Set by `applyTheme` through `styleTheme`. */
let active: Palette = LIGHT;

/** Switch the table when the interface theme changes. */
export function styleTheme(name: 'dark' | 'light'): void {
  active = name === 'dark' ? DARK : LIGHT;
}

/** The style for an element, from the stereotype the projection carries. */
export function forStereotype(stereotype: string): BoxStyle {
  const kind = stereotype.trim().replace(/^«/, '').replace(/»$/, '').trim();
  return active[kind] ?? forGroup(kind);
}

/**
 * The kinds `sysml_view::style` groups onto one entry.
 *
 * Kept as a second step so the two tables each hold one row per colour rather than one row per
 * spelling: `port def` and `interface def` are the same blue, and a table that listed them
 * separately is a table where one of them will be updated and the other will not.
 */
function forGroup(kind: string): BoxStyle {
  switch (kind) {
    case 'part definition':
    case 'port def':
    case 'port definition':
    case 'interface def':
    case 'connection def':
    case 'allocation def':
      return active['part def'];
    case 'interface':
    case 'connection':
    case 'allocation':
      return active.part;
    case 'enumeration def':
    case 'enumeration':
      return active.enum;
    case 'state def':
      return active.state;
    case 'action def':
    case 'accept action':
    case 'send action':
    case 'perform':
      return active.action;
    case 'metadata def':
      return active.metadata;
    case 'doc':
    case 'rep':
      return active.comment;
    case 'namespace':
    case 'library package':
      return active.package;
    case 'item def':
      return active.item;
    case 'require constraint':
    case 'assume constraint':
      return active['constraint def'];
    default:
      return PLAIN;
  }
}

/**
 * The ink a name needs on a header band.
 *
 * A header is either a pale tint or a saturated one, and one text colour cannot read on both.
 * The test is the band's luminance, the same one the kernel uses, so a colour added to the table
 * cannot make a name unreadable.
 */
export function headerInk(style: BoxStyle): string {
  return luminance(style.header) < 0.55 ? '#FFFFFF' : '#1A1A1A';
}

function luminance(colour: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(colour.trim());
  if (!match) return 1;
  const value = Number.parseInt(match[1], 16);
  const r = (value >> 16) & 0xff;
  const g = (value >> 8) & 0xff;
  const b = value & 0xff;
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/** Whether an element of this kind is drawn as a person rather than as a box. */
export function isActor(stereotype: string): boolean {
  const kind = stereotype.trim().replace(/^«/, '').replace(/»$/, '').trim();
  return kind === 'actor';
}

/** Local display dimensions shared with the SVG renderer, not normative pixel measurements. */
export function actorFigure(cx: number, top: number, ink: string): string {
  const headCy = top + 10;
  const bodyTop = top + 17;
  const bodyBottom = top + 62;
  const armY = top + 35;
  const legY = top + 96;
  return [
    `<circle cx="${cx}" cy="${headCy}" r="7" fill="none" stroke="${ink}" stroke-width="0.5"/>`,
    `<path d="M${cx},${bodyTop} L${cx},${bodyBottom} ` +
      `M${cx - 19},${armY} L${cx + 19},${armY} ` +
      `M${cx},${bodyBottom} L${cx - 17},${legY} ` +
      `M${cx},${bodyBottom} L${cx + 17},${legY}" ` +
      `fill="none" stroke="${ink}" stroke-width="0.5"/>`,
  ].join('');
}
