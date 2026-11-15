import { build, context } from 'esbuild';
import { execFileSync, spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const watch = process.argv.includes('--watch');

// Tailwind v4 compiles the token layer and the utilities into one plain stylesheet, which esbuild
// then folds into `bundle.css` alongside Monaco's own CSS. Two steps because esbuild does not run
// PostCSS plugins; keeping the output an ordinary file leaves the rest of the pipeline untouched.
const tailwindCli = path.join(here, 'node_modules', '@tailwindcss', 'cli', 'dist', 'index.mjs');
const tailwindArgs = ['-i', 'src/tailwind.css', '-o', 'src/styles.generated.css'];

if (watch) {
  const tailwind = spawn(process.execPath, [tailwindCli, ...tailwindArgs, '--watch'], {
    cwd: here,
    stdio: 'inherit',
  });
  process.on('exit', () => tailwind.kill());
} else {
  execFileSync(process.execPath, [tailwindCli, ...tailwindArgs, '--minify'], {
    cwd: here,
    stdio: 'inherit',
  });
}

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
  // One mapping for `@/`, so the compiler and the bundler resolve to the same absolute paths.
  // Two aliases for one module is how a second React instance gets built, and the symptom is a
  // blank page from a stale context rather than an error.
  alias: { '@': path.join(here, 'src') },
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
