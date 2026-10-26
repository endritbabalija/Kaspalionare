// S3 round detail (spec 3 S3, S4; J4, J6): m, S, J, the deadline T, D, the void lock and the claim deadline (clock
// and DAA), the anchor block, the winning numbers (final from a live pool; provisional, labelled, before the draw,
// DECISIONS 2026-10-26), the k frozen roots, the live pools and what is left in them, the vault settlement record,
// the round's transactions; then Verify in this browser and Run the round. A figure a node-checked object fixes
// links to the transaction holding it; one only the indexer states says "Unverified". Nothing is computed here:
// the figures are the round's facts (Rust) and its objects' decoded states.
import type { Combination, RecordView } from '@wasm/web.js';
import { useSession } from '@/app/context';
import { Amount } from '@/components/Amount';
import { BallRow } from '@/components/BallRow';
import { CallerPanel } from '@/components/CallerPanel';
import { FigureRow } from '@/components/FigureRow';
import { HashOrAddress } from '@/components/HashOrAddress';
import { RoundStatusPill } from '@/components/RoundStatusPill';
import { SourceLink } from '@/components/SourceLink';
import { Time } from '@/components/Time';
import { Unverified } from '@/components/Unverified';
import { VerifyPanel } from '@/components/VerifyPanel';
import { groupDigits } from '@/lib/format';
import { objectsOf, type DetailData } from '@/rounds/load';
import { useWallet } from '@/wallet/context';
import { useRoundDetail } from './useRoundDetail';

const card = 'flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4';

function Value({
  children,
  checked,
  source,
  name,
  testId,
}: {
  children: React.ReactNode;
  checked: boolean;
  source?: string;
  name: string;
  testId: string;
}) {
  const { config } = useSession();
  return (
    <span className="flex min-w-0 items-center gap-1" data-testid={testId} data-checked={checked ? 'true' : 'false'}>
      {children}
      {checked ? (
        <SourceLink tx={source} name={name} explorer={config.explorerUrl} testId={`${testId}-source`} />
      ) : (
        <Unverified testId={`${testId}-unverified`} />
      )}
    </span>
  );
}

export function RoundDetail({ r }: { r: string }) {
  const ctx = useSession();
  const wallet = useWallet();
  const d = useRoundDetail(ctx, wallet, r);
  const title = (
    <a href="#/rounds" data-testid="rounds-back" className="flex min-h-11 items-center text-[13px] font-medium">
      Rounds
    </a>
  );
  if (d.load.kind === 'loading') {
    return (
      <section
        data-testid="round-detail"
        data-state="loading"
        aria-busy="true"
        aria-label={`Round ${r}`}
        className="flex flex-col gap-3"
      >
        {title}
        <h1 className="m-0 text-[17px] font-semibold">Round {r}</h1>
        <span className="h-64 w-full animate-pulse rounded-[14px] bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (d.load.kind === 'error') {
    return (
      <section data-testid="round-detail" data-state="error" aria-label={`Round ${r}`} className="flex flex-col gap-3">
        {title}
        <h1 className="m-0 text-[17px] font-semibold">Round {r}</h1>
        <p role="alert" data-testid="round-detail-error" data-code={d.load.code ?? ''} className="m-0 text-amber">
          {d.load.message}
        </p>
        <VerifyPanel state={d.verify.state} result={d.verify.result} error={d.verify.error} onVerify={d.onVerify} />
      </section>
    );
  }
  const data = d.load.data;
  return (
    <section
      data-testid="round-detail"
      data-state="ready"
      data-round={r}
      data-status={data.status ?? ''}
      aria-labelledby="round-detail-title"
      className="flex flex-col gap-3.5"
    >
      {title}
      <div className="flex items-center gap-2">
        <h1 id="round-detail-title" className="m-0 text-[17px] font-semibold">
          Round {r}
        </h1>
        {data.status ? <RoundStatusPill status={data.status} /> : <Unverified testId="round-detail-status" />}
      </div>
      <Figures
        data={data}
        anchorBound={d.verify.result?.anchorBound ? d.verify.result.winning?.anchorHash : undefined}
      />
      <Roots data={data} />
      <Pools data={data} />
      <Settlement data={data} />
      <VerifyPanel state={d.verify.state} result={d.verify.result} error={d.verify.error} onVerify={d.onVerify} />
      <CallerPanel
        round={r}
        due={d.due}
        reward={data.checked.facts.reward?.value}
        figures={d.caller.prep?.figures}
        estimating={d.caller.estimating}
        error={d.caller.error}
        tx={d.caller.tx}
        walletConnected={d.walletConnected}
        disabled={d.running || d.blocked}
        onPress={d.onPress}
        clock={data.clock}
        explorer={ctx.config.explorerUrl}
      />
    </section>
  );
}

function Figures({ data, anchorBound }: { data: DetailData; anchorBound?: string }) {
  const { config } = useSession();
  const f = data.checked.facts;
  const api = data.api;
  const clock = data.clock;
  const time = (daa: string, testId: string) => (
    <Time daa={daa} virtualDaa={clock.virtualDaa} observedAt={clock.observedAt} testId={testId} />
  );
  const pool = objectsOf(data.checked, 'pool')[0]?.view;
  const final: Combination | undefined = pool?.role === 'pool' ? pool.win : undefined;
  const numbers = final ?? data.provisional?.winning ?? api.draw?.winning;
  const numbersState = final ? 'final' : data.provisional ? 'provisional' : numbers ? 'unverified' : '';
  // X.hash: bound to a public node's block (the closed path, or Verify after the draw), else the indexer's.
  const anchor = data.provisional?.anchorHash ?? anchorBound;
  const anchorShown = anchor ?? (api.draw && !api.draw.void ? api.draw.anchorHash : undefined);
  const anchorDaa = data.provisional?.anchorDaaScore;
  const sales = f.sales
    ? { v: f.sales.value, c: true, s: f.sales.sources[0]?.transactionId }
    : data.row?.sales
      ? { v: data.row.sales, c: false }
      : undefined;
  const jackpot = f.jackpot
    ? { v: f.jackpot.value, c: true, s: f.jackpot.sources[0]?.transactionId }
    : data.row?.jackpot
      ? { v: data.row.jackpot, c: false }
      : undefined;
  const deadline = f.deadline
    ? { v: f.deadline.value, c: true, s: f.deadline.sources[0]?.transactionId }
    : api.deadline
      ? { v: api.deadline, c: false }
      : undefined;
  const dd = f.d
    ? { v: f.d.value, c: true, s: f.d.sources[0]?.transactionId }
    : api.anchorDaaScore
      ? { v: api.anchorDaaScore, c: false }
      : undefined;
  const claimBy = f.claimDeadline
    ? { v: f.claimDeadline.value, c: true, s: f.claimDeadline.sources[0]?.transactionId }
    : api.draw?.claimDeadline && !api.draw.void
      ? { v: api.draw.claimDeadline, c: false }
      : undefined;
  const m = data.checked.m;
  return (
    <div className={card} data-testid="round-figures">
      {m !== undefined && (
        <FigureRow name="m" testId="round-m-row">
          <Value checked={data.checked.statusChecked} name="m" testId="round-m">
            <span data-value={m}>{m}</span>
          </Value>
        </FigureRow>
      )}
      {sales && (
        <FigureRow name="Sales" testId="round-sales-row">
          <Value checked={sales.c} source={sales.s} name="Sales" testId="round-sales">
            <Amount sompi={sales.v} testId="round-sales-amount" />
          </Value>
        </FigureRow>
      )}
      {jackpot && (
        <FigureRow name="Jackpot" testId="round-jackpot-row">
          <Value checked={jackpot.c} source={jackpot.s} name="Jackpot" testId="round-jackpot">
            <Amount sompi={jackpot.v} testId="round-jackpot-amount" />
          </Value>
        </FigureRow>
      )}
      {deadline && (
        <FigureRow name="Deadline" testId="round-deadline-row">
          <Value checked={deadline.c} source={deadline.s} name="Deadline" testId="round-deadline">
            {time(deadline.v, 'round-deadline-time')}
          </Value>
        </FigureRow>
      )}
      {dd && (
        <FigureRow name="D" testId="round-d-row">
          <Value checked={dd.c} source={dd.s} name="D" testId="round-d">
            {time(dd.v, 'round-d-time')}
          </Value>
        </FigureRow>
      )}
      {f.voidFrom && (
        <FigureRow name="Void from" testId="round-void-row">
          <Value checked source={f.voidFrom.sources[0]?.transactionId} name="Void from" testId="round-void">
            {time(f.voidFrom.value, 'round-void-time')}
          </Value>
        </FigureRow>
      )}
      {claimBy && (
        <FigureRow name="Claim by" testId="round-claim-row">
          <Value checked={claimBy.c} source={claimBy.s} name="Claim by" testId="round-claim">
            {time(claimBy.v, 'round-claim-time')}
          </Value>
        </FigureRow>
      )}
      {anchorShown && (
        <FigureRow name="Anchor block" testId="round-anchor-row">
          <span
            className="flex min-w-0 items-center gap-1"
            data-testid="round-anchor"
            data-checked={anchor ? 'true' : 'false'}
          >
            <HashOrAddress value={anchorShown} kind="block" explorer={config.explorerUrl} testId="round-anchor-hash" />
            {!anchor && <Unverified testId="round-anchor-unverified" />}
          </span>
        </FigureRow>
      )}
      {anchorDaa && (
        <FigureRow name="Anchor DAA" testId="round-anchor-daa-row" part>
          <span data-testid="round-anchor-daa" data-value={anchorDaa}>
            {groupDigits(anchorDaa)}
          </span>
        </FigureRow>
      )}
      {api.draw?.void && data.status === 'void' && (
        <FigureRow name="Draw" testId="round-void-status-row">
          <span data-testid="round-void-status">No draw: void</span>
        </FigureRow>
      )}
      {numbers && (
        <div className="flex flex-col gap-1.5 py-2" data-testid="round-numbers" data-state={numbersState}>
          <div className="flex items-center justify-between gap-2 text-sm">
            <span>Winning numbers</span>
            {numbersState === 'final' && (
              <span data-testid="round-numbers-status" className="font-mono text-[13px]">
                Final
              </span>
            )}
            {numbersState === 'provisional' && (
              <span data-testid="round-numbers-status" className="font-mono text-[13px] text-amber">
                Provisional
              </span>
            )}
            {numbersState === 'unverified' && <Unverified testId="round-numbers-status" />}
          </div>
          <BallRow combination={numbers} testId="round-numbers-balls" />
        </div>
      )}
      <Transactions data={data} />
    </div>
  );
}

function Transactions({ data }: { data: DetailData }) {
  const { config } = useSession();
  const close = data.api.close?.transactionId;
  const draw = data.api.draw?.transactionId;
  return (
    <>
      {close && (
        <FigureRow name="close()" testId="round-close-tx-row">
          <HashOrAddress value={close} kind="transaction" explorer={config.explorerUrl} testId="round-close-tx" />
        </FigureRow>
      )}
      {draw && (
        <FigureRow name={data.api.draw?.void ? 'Void' : 'draw()'} testId="round-draw-tx-row">
          <HashOrAddress value={draw} kind="transaction" explorer={config.explorerUrl} testId="round-draw-tx" />
        </FigureRow>
      )}
    </>
  );
}

function Roots({ data }: { data: DetailData }) {
  const { config } = useSession();
  const f = data.checked.facts;
  if (!f.roots) return null;
  return (
    <div className={card} data-testid="round-roots" data-count={f.roots.length}>
      <div className="flex min-h-11 items-center justify-between gap-2">
        <h2 className="m-0 text-[15px] font-semibold">Frozen roots</h2>
        <SourceLink
          tx={f.rootsSource?.transactionId}
          name="Frozen roots"
          explorer={config.explorerUrl}
          testId="round-roots-source"
        />
      </div>
      <ol className="m-0 grid list-none grid-cols-1 gap-x-3 p-0">
        {f.roots.map((root, i) => (
          <li key={i} data-testid={`round-root-${i}`} className="flex items-center gap-2">
            <span className="w-5 font-mono text-[12px] text-graphite">{i}</span>
            <HashOrAddress value={root} kind="block" testId={`round-root-${i}-hash`} />
          </li>
        ))}
      </ol>
    </div>
  );
}

function Pools({ data }: { data: DetailData }) {
  const { config } = useSession();
  const pools = data.checked.facts.pools;
  if (!pools.length) return null;
  return (
    <div className={card} data-testid="round-pools" data-count={pools.length}>
      <h2 className="m-0 pb-1 text-[15px] font-semibold">Pools</h2>
      {pools.map((p, i) => (
        <div key={`${p.outpoint.transactionId}:${p.outpoint.index}`} data-testid={`round-pool-${i}`} data-tier={p.tier}>
          <FigureRow name={p.tierName} testId={`round-pool-${i}-row`}>
            <span className="flex items-center gap-1">
              <Amount sompi={p.remaining} testId={`round-pool-${i}-remaining`} />
              <SourceLink
                tx={p.outpoint.transactionId}
                name={`Pool ${p.tierName}`}
                explorer={config.explorerUrl}
                testId={`round-pool-${i}-source`}
              />
            </span>
          </FigureRow>
          <FigureRow name="Slice" testId={`round-pool-${i}-slice-row`} part>
            <Amount sompi={p.slice} testId={`round-pool-${i}-slice`} />
          </FigureRow>
        </div>
      ))}
    </div>
  );
}

function Settlement({ data }: { data: DetailData }) {
  const { config } = useSession();
  const rec = objectsOf(data.checked, 'record')[0];
  if (!rec || rec.view.role !== 'record') return null;
  const v: RecordView = rec.view;
  return (
    <div className={card} data-testid="round-settlement">
      <div className="flex min-h-11 items-center justify-between gap-2">
        <h2 className="m-0 text-[15px] font-semibold">Vault settlement</h2>
        <SourceLink
          tx={rec.object.utxo.transactionId}
          name="Vault settlement"
          explorer={config.explorerUrl}
          testId="round-settlement-source"
        />
      </div>
      <FigureRow name="Refund mode" testId="round-settlement-refund-row">
        <span data-testid="round-settlement-refund">{v.refund === 1 ? 'Yes' : 'No'}</span>
      </FigureRow>
      <FigureRow name="Deposits left" testId="round-settlement-dep-row">
        <Amount sompi={v.depRem} testId="round-settlement-dep" />
      </FigureRow>
      <FigureRow name="Shares to mint" testId="round-settlement-mint-row">
        <span data-testid="round-settlement-mint">{groupDigits(v.mintRem)}</span>
      </FigureRow>
      <FigureRow name="Withdrawal shares left" testId="round-settlement-wsh-row">
        <span data-testid="round-settlement-wsh">{groupDigits(v.wshRem)}</span>
      </FigureRow>
      <FigureRow name="Shares to burn" testId="round-settlement-burn-row">
        <span data-testid="round-settlement-burn">{groupDigits(v.burnRem)}</span>
      </FigureRow>
      <FigureRow name="Payouts left" testId="round-settlement-out-row">
        <Amount sompi={v.outRem} testId="round-settlement-out" />
      </FigureRow>
    </div>
  );
}
