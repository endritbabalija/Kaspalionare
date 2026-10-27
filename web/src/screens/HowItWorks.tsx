// S6 How it works (spec 3 S6, D138; the one page with explanations, spec 5 rule 6): the draw-cabinet figure in its
// own frame, then the four calls, where the money goes, why counts freeze before the seed, where the numbers come
// from (and the open miner-withholding item, D100), prizes, deposits and voids, what the page checks, the backer
// vault, and the testnet beta. Every number of the example round comes from how/cabinet-model.json, which the Rust
// lottery code writes (crates/lottery/tests/f6_figure.rs); this file only prints it.
import type { ReactNode } from 'react';
import { Amount } from '@/components/Amount';
import { FigureFrame } from '@/components/FigureFrame';
import { FigureRow } from '@/components/FigureRow';
import { MoneySplit, type SplitPart } from '@/components/MoneySplit';
import { daaSpan, groupDigits, kas2, percentOfBp } from '@/lib/format';
import type { Theme } from '@/theme/theme';
import model from './how/cabinet-model.json';

const C = model.constants;
const T = model.perTicket;
const pad = (n: number) => n.toString().padStart(2, '0');

function Part({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section
      data-testid={`how-${id}`}
      aria-labelledby={`how-${id}-title`}
      className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4 text-[15px] leading-relaxed [&_p]:m-0"
    >
      <h2 id={`how-${id}-title`} className="m-0 text-[15px] font-semibold">
        {title}
      </h2>
      {children}
    </section>
  );
}

const Call = ({ name }: { name: string }) => <code className="font-mono text-[14px] text-ink">{name}</code>;

export function HowItWorks({ theme }: { theme: Theme }) {
  const parts: SplitPart[] = [
    { key: 'house', name: 'House vault', sompi: T.H, fill: 'bg-teal' },
    { key: 'tiers', name: 'Lower prizes', sompi: T.Bsum, fill: 'bg-mint' },
    { key: 'referral', name: 'Referral', sompi: T.R, fill: 'bg-ink' },
    { key: 'fee', name: 'Protocol fee', sompi: T.F, fill: 'bg-amber' },
    { key: 'close', name: 'close() caller', sompi: T.C_close, fill: 'bg-graphite' },
    { key: 'draw', name: 'draw() caller', sompi: T.C_draw, fill: 'bg-control-line' },
    { key: 'boost', name: 'Boost pot', sompi: T.b, fill: 'bg-status-closing-fg' },
  ].filter((p) => p.sompi !== '0');
  const price = kas2(model.price);
  const deposit = kas2(model.deposit);
  const pMin = kas2(C.pMin);
  const minDeposit = kas2(C.minDeposit);
  return (
    <section data-testid="how" aria-labelledby="how-title" className="flex flex-col gap-3.5">
      <h1 id="how-title" className="m-0 text-[17px] font-semibold">
        How it works
      </h1>

      <figure className="m-0 flex flex-col gap-2" data-testid="how-figure">
        <figcaption className="text-[15px] font-semibold">The draw cabinet: round {model.round}, an example</figcaption>
        <FigureFrame
          key={theme}
          src={theme === 'light' ? '/figures/draw-cabinet-tour-light.html' : '/figures/draw-cabinet-tour.html'}
          figure="draw-cabinet"
          title={`The draw cabinet: a guided tour of example round ${model.round}`}
          testId="how-figure-frame"
        />
      </figure>

      <Part id="calls" title="One round, four calls">
        <p>
          <Call name="buy()" /> You pick 5 numbers from 1 to 30 and a bonus number, up to {C.bMax} combinations on one
          ticket. Each costs {price} KAS plus a {deposit} KAS deposit. The ticket is yours, on chain.
        </p>
        <p>
          <Call name="close()" /> After the sales deadline anyone can close the round. It seals all {model.k} shards at
          once and opens the next round.
        </p>
        <p>
          <Call name="draw()" /> About {daaSpan(C.seedOffsetDaa)} later anyone can draw. The draw sets the winning
          numbers, funds every prize pool and fixes the next round's jackpot.
        </p>
        <p>
          <Call name="claim()" /> A winning ticket claims its prize, paid to the ticket's own payout address.
        </p>
        <p>
          Anyone can run <Call name="close()" /> and <Call name="draw()" />: each pays its caller{' '}
          {percentOfBp(model.shares.C)} of the round's sales. On this beta sales are small, so that reward is usually
          below the network fee, and the site's owner runs both.
        </p>
      </Part>

      <Part id="money" title={`Where ${price} KAS goes`}>
        <MoneySplit parts={parts} testId="how-split" />
        <p>
          The house vault belongs to its backers and pays the jackpot's house part and the second prizes. Lower prizes
          come from the round's own sales. The referral goes to the address in your referral link, or to the site's
          owner; below {kas2(C.mRef)} KAS it stays in the round. Unclaimed prizes, unpaid referrals and roundings go to
          the boost pot, which only ever adds to later jackpots.
        </p>
        <FigureRow name={`Round ${model.round} sales`} testId="how-example-sales">
          <Amount sompi={model.sales} testId="how-example-sales-amount" />
        </FigureRow>
        <FigureRow name="To the house vault" testId="how-example-house">
          <Amount sompi={model.amounts.H} testId="how-example-house-amount" />
        </FigureRow>
        <FigureRow name="To each caller" testId="how-example-caller">
          <Amount sompi={model.amounts.C_close} testId="how-example-caller-amount" />
        </FigureRow>
      </Part>

      <Part id="frozen" title="Why sales stop before the draw">
        <p>
          <Call name="close()" /> takes in all {model.k} shards in one transaction, so every ticket count is fixed. The
          block that picks the numbers comes only after that: the first chain block at least{' '}
          {groupDigits(String(C.seedOffsetDaa))} DAA (about {daaSpan(C.seedOffsetDaa)}) after the close. Nobody can buy
          into a draw they already know.
        </p>
      </Part>

      <Part id="seed" title="Where the numbers come from">
        <p>
          The winning numbers come from the hash of that block, the anchor block, mixed with the round. The draw proves
          on chain that it used the right block, so nobody can choose another one.
        </p>
        <p data-testid="how-open-item">
          Open item: the miner who finds the anchor block sees the numbers first, and could throw the block away to try
          again. That costs a block reward and pays only for a miner with a large share of the network. The fix, a time
          delay proven on chain, is required before any real-money version and is not built. Until then the draw is not
          fully fair against such a miner.
        </p>
        <FigureRow name={`Round ${model.round} numbers`} testId="how-example-numbers">
          <span className="font-mono">
            {model.win.map(pad).join(' ')} + {pad(model.bonus)}
          </span>
        </FigureRow>
        <FigureRow name="Jackpot odds" testId="how-example-odds">
          <span className="font-mono">
            1 in {groupDigits(String(C.combinations))} × {model.m}
          </span>
        </FigureRow>
      </Part>

      <Part id="prizes" title="Prizes, deposits and voids">
        <p>
          You have at least {daaSpan(C.claimWindowDaa)} after the draw to claim. Tickets holding the same winning
          combination share its prize equally, whoever claims first.
        </p>
        <p>A prize below {pMin} KAS is not paid alone: it stays on the ticket and is paid with its last claim.</p>
        <p>
          Each combination's {deposit} KAS deposit comes back with the ticket's last claim. A ticket with nothing left
          to claim gets it back with Return deposit once the claim window has passed.
        </p>
        <p>
          If nobody draws within {daaSpan(C.drawWindowDaa)} of the anchor point, anyone can void the round. A void has
          no winners; the round's prize money goes to later jackpots. Ticket prices are not refunded; deposits come back
          with Return deposit from {daaSpan(C.returnAfterDaa)} after the sales deadline.
        </p>
        <FigureRow name={`Round ${model.round} jackpot`} testId="how-example-jackpot">
          <Amount sompi={model.vault.J} testId="how-example-jackpot-amount" />
        </FigureRow>
        <FigureRow name={`Shared by ${model.h} tickets, each`} testId="how-example-prize">
          <Amount sompi={model.prize} testId="how-example-prize-amount" />
        </FigureRow>
      </Part>

      <Part id="checks" title="What the page checks">
        <p>
          The page reads rounds and tickets from an indexer, a helper server, and checks each object against a public
          Kaspa node before it uses it. A wrong indexer can stop the page from building a transaction; it cannot move
          money.
        </p>
        <p>
          <strong className="font-semibold">Verify in this browser</strong>, on a round's page, rebuilds the {model.k}{' '}
          shard trees from the purchases and compares them with the roots frozen on chain, recomputes the winning
          numbers from the anchor block's hash, and compares each object's state with the live chain.
        </p>
        <p>
          <strong className="font-semibold">Provisional</strong>: winning numbers worked out from the anchor block
          before <Call name="draw()" /> has put them on chain. They become final with the draw.
        </p>
        <p>
          <strong className="font-semibold">Unverified</strong>: a figure only the indexer states, which no live chain
          object confirms.
        </p>
      </Part>

      <Part id="vault" title="Backing the vault">
        <p>
          Backers own the house vault through shares. A deposit (at least {minDeposit} KAS) or a withdrawal is a
          request: it cannot be cancelled, and it converts at the price the next draw sets, after that round's prizes
          are paid. Withdrawals never take the vault below what the next jackpot needs; the rest waits for a later draw.
          If deposits would buy less than one share each, the draw refunds them instead (refund mode).
        </p>
        <p data-testid="how-vault-indexer">
          The page finds your positions through the indexer. If it leaves one out, the position is still safe on chain:
          only your key can move it, and it can be withdrawn with another indexer or the repository's tools.
        </p>
        <FigureRow name={`Share price, before round ${model.round}`} testId="how-example-price-before">
          <span className="font-mono">{model.backers.price0} sompi/share</span>
        </FigureRow>
        <FigureRow name="After its draw" testId="how-example-price-after">
          <span className="font-mono">{model.backers.priceDraw} sompi/share</span>
        </FigureRow>
      </Part>

      <Part id="beta" title="A testnet beta">
        <p>
          This site runs on Kaspa testnet-10. Its coins have no value. The rules are enforced by the chain, but the code
          has not been audited, and the open item above is not fixed.
        </p>
      </Part>
    </section>
  );
}
