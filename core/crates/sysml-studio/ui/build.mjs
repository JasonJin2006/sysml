import { build, context } from 'esbuild';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const watch = process.argv.includes('--watch');

// The bundle is committed and embedded into the server binary (`include_str!`), so this script is
// the only build step: a browser cannot resolve a bare `react` import, and the studio must work
// from a binary with no toolchain installed.
//
// Minified: the artifact is generated code and nobody reads it in review — the source next to it
// is what gets read. `--watch` keeps it readable for a bundle someone is actually debugging.
const options = {
  absWorkingDir: here,
  entryPoints: ['src/main.tsx'],
  bundle: true,
  // `outdir` rather than `outfile`: Monaco imports its own CSS, and esbuild writes an imported
  // stylesheet beside the script only when it has a directory to put it in. `entryNames` and
  // `assetNames` keep the two names the server embeds (`bundle.js`, `bundle.css`).
  outdir: '../assets/dist',
  entryNames: 'bundle',
  assetNames: 'assets/[name]',
  jsx: 'automatic',
  target: ['es2020'],
  // Monaco ships `.ttf` for the codicon font; inlined so the bundle stays the only file the
  // server has to carry.
  loader: { '.ttf': 'dataurl', '.woff': 'dataurl', '.woff2': 'dataurl' },
  minify: !watch,
  sourcemap: false,
  legalComments: 'none',
  logLevel: 'info',
};

if (watch) {
  const ctx = await context(options);
  await ctx.watch();
} else {
  await build(options);
}
