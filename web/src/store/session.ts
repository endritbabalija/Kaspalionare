// The page's one local store (spec 6.3), opened on first use with the layout crates/web declares.
import type { Ctx } from '@/net/round';
import { WebStore } from './webstore';

const stores = new WeakMap<Ctx, Promise<WebStore>>();

export function storeOf(ctx: Ctx): Promise<WebStore> {
  let p = stores.get(ctx);
  if (!p) {
    p = ctx.lottery.call('webstoreInfo').then((info) => WebStore.browser(info));
    p.catch(() => stores.delete(ctx));
    stores.set(ctx, p);
  }
  return p;
}
