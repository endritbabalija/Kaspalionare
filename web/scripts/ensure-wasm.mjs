// `npm test` needs the generated `.d.ts` (the page's only type source): build the dev wasm package if
// it is missing (a fresh clone). Later runs leave it as it is; `npm run wasm:dev` rebuilds it.
import { existsSync } from 'node:fs';
import path from 'node:path';
import { WEB, run } from './lib.mjs';

const dts = path.join(WEB, 'src', 'wasm', 'dev', 'web.d.ts');
if (!existsSync(dts)) {
  console.log('src/wasm/dev is missing: building it (crates/web, features simnet,node; a few minutes the first time)');
  run(process.execPath, [path.join(WEB, 'scripts', 'wasm.mjs'), 'dev'], { cwd: WEB });
}
