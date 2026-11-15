/**
 * Save the picture as a standalone SVG.
 *
 * The scene is drawn with classes, so a bare copy of its markup would render
 * unstyled. The rules below are the canvas portion of styles.css with the
 * active theme's variables resolved to literal colours, embedded in the file
 * so it opens correctly outside the application.
 */

type PaletteValue = (name: string, fallback: string) => string;

function palette(): PaletteValue {
  const computed = getComputedStyle(document.documentElement);
  return (name, fallback) => computed.getPropertyValue(name).trim() || fallback;
}

export function exportSceneSvg(): boolean {
  const scene = document.getElementById('scene');
  if (!(scene instanceof SVGSVGElement)) return false;

  const v = palette();
  const node = v('--ui-node-fill', '#141417');
  const canvas = v('--ui-canvas', '#0B0C0F');
  const border = v('--ui-border', '#242629');
  const text = v('--ui-text', '#F2F3F5');
  const text2 = v('--ui-text2', '#C9CBD1');
  const text3 = v('--ui-text3', '#999EA6');
  const accent = v('--ui-accent', '#5E6AD2');
  const edge = v('--ui-edge', '#4A4D54');
  const faint = v('--ui-text5', '#54595E');
  const teal = v('--ui-teal', '#26A699');
  const amber = v('--ui-amber', '#F5A624');
  const red = v('--ui-red', '#FF4D4F');
  const redEdge = v('--ui-red-edge', '#7A4040');

  const css = [
    `.node-body{fill:${node};stroke:${border};stroke-width:.5}`,
    `.node.selected .node-body{stroke:${accent};stroke-width:2}`,
    `.node-resize{fill:${node};stroke:${text};stroke-width:1;opacity:0}`,
    `.node .label{font-size:10px;font-weight:700;fill:${text}}`,
    `.node .stereo{font-size:9px;font-style:italic;fill:${text3}}`,
    `.node .row{font-size:10px;fill:${text2}}`,
    `.node .constraint{font-size:9.5px;fill:${text3}}`,
    `.node .port-name{font-size:9.5px;fill:${text2}}`,
    `.node.unsure .node-body{stroke-dasharray:4 3;stroke:${redEdge}}`,
    `.frame-body{fill:${canvas};stroke:${border};stroke-width:1}`,
    `.frame-rule{stroke:${border};stroke-width:.8}`,
    `.frame-title{font-size:12px;fill:${text}}`,
    `.frame.selected .frame-body{stroke:${accent};stroke-width:2}`,
    `.edge{fill:none;stroke:${edge};stroke-width:1}`,
    `.edge.hit{stroke:transparent;stroke-width:10;fill:none}`,
    `.edge.satisfy{stroke-width:1}`,
    `.edge.annotate{stroke-width:1;stroke-dasharray:5 3}`,
    `.edge.connector{stroke-width:2.6}`,
    `.edge-label{font-size:9px;fill:${text3}}`,
    `.judge circle{stroke-width:1}`,
    `.judge text{font-size:9px;fill:#fff;font-weight:600}`,
    `.judge.pass circle{fill:${teal};stroke:${teal}}`,
    `.judge.fail circle,.judge.error circle{fill:${red};stroke:${red}}`,
    `.judge.inconclusive circle{fill:${amber};stroke:${amber}}`,
    `.judge.stale circle{fill:${node};stroke-dasharray:2 2}`,
    `.judge.stale text{fill:${text}}`,
    `.judge.stale.pass circle{stroke:${teal}}`,
    `.judge.stale.fail circle,.judge.stale.error circle{stroke:${red}}`,
    `.judge.stale.inconclusive circle{stroke:${amber}}`,
    `.mark-new{fill:none;stroke:${teal};stroke-width:1.5;stroke-dasharray:4 3}`,
    `.mark-ghost rect{fill:${node};stroke:${faint};stroke-width:1;stroke-dasharray:3 3;opacity:.75}`,
    `.mark-ghost .ghost-label{font-size:12px;fill:${faint};text-decoration:line-through}`,
    `.mark-change rect{fill:${node};stroke:${amber};stroke-width:.8}`,
    `.mark-change text{font-size:10px;fill:${amber}}`,
  ].join('\n');

  const clone = scene.cloneNode(true) as SVGSVGElement;
  const computed = getComputedStyle(document.documentElement);
  for (const name of Array.from(computed)) {
    if (name.startsWith('--ui-') || ['--fg', '--panel', '--dim', '--faint', '--line', '--node-border'].includes(name)) {
      clone.style.setProperty(name, computed.getPropertyValue(name));
    }
  }
  clone.style.fontFamily = v('--ui-font', 'Segoe UI, Arial, sans-serif');
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  clone.setAttribute('width', String(scene.clientWidth || 1024));
  clone.setAttribute('height', String(scene.clientHeight || 768));
  clone.setAttribute('viewBox', `0 0 ${scene.clientWidth || 1024} ${scene.clientHeight || 768}`);
  const style = document.createElementNS('http://www.w3.org/2000/svg', 'style');
  style.textContent = css;
  clone.insertBefore(style, clone.firstChild);

  const markup = `<?xml version="1.0" encoding="UTF-8"?>\n${new XMLSerializer().serializeToString(clone)}`;
  const url = URL.createObjectURL(new Blob([markup], { type: 'image/svg+xml;charset=utf-8' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = 'sysml-model.svg';
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
  return true;
}
