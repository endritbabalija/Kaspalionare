// The negative control of check-bundle.mjs (F2/F3 review note): a development build of the same page
// (`vite build --mode development`, import.meta.env.DEV true) into dist-dev-check/, which must contain every
// dev marker the production check looks for. If a marker were tree-shaken even here, its absence from dist/
// would prove nothing. Run by `npm run build` after check-bundle.mjs.
import { rmSync } from 'node:fs';
import path from 'node:path';
import { WEB, run } from './lib.mjs';

const out = path.join(WEB, 'dist-dev-check');
rmSync(out, { recursive: true, force: true });
try {
  run(
    process.execPath,
    [
      path.join(WEB, 'node_modules', 'vite', 'bin', 'vite.js'),
      'build',
      '--mode',
      'development',
      '--outDir',
      out,
      '--emptyOutDir',
    ],
    {
      cwd: WEB,
      env: {
        ...process.env,
        NODE_ENV: 'development',
        VITE_INDEXER_URL: process.env.VITE_INDEXER_URL ?? 'https://indexer.invalid/v1/',
        VITE_COVENANT_ID: process.env.VITE_COVENANT_ID ?? '00'.repeat(32),
      },
    },
  );
  run(process.execPath, [path.join(WEB, 'scripts', 'check-bundle.mjs'), '--control', out], { cwd: WEB });
} finally {
  rmSync(out, { recursive: true, force: true });
}
