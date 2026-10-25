// The component catalogue (spec 1.1 DX): every base component in every state, in both themes side by
// side. Development builds only (#/dev/catalogue; App.tsx imports this file behind import.meta.env.DEV,
// and scripts/check-bundle.mjs checks that a production bundle has no trace of it). The values are
// sample figures for the catalogue only.
import type { ReactNode } from 'react';
import { Amount } from '@/components/Amount';
import { BonusTile, type BonusTileState } from '@/components/BonusTile';
import { CostPanel } from '@/components/CostPanel';
import { HashOrAddress } from '@/components/HashOrAddress';
import { JackpotCountdown } from '@/components/JackpotCountdown';
import { NumberBall, type NumberBallState } from '@/components/NumberBall';
import { RoundStatusPill } from '@/components/RoundStatusPill';
import { Time } from '@/components/Time';
import { TicketRow } from '@/components/TicketRow';
import { TxProgress } from '@/components/TxProgress';
import { RoundCardView } from '@/screens/play/RoundCard';

/** Searched for by scripts/check-bundle.mjs: present in development, absent from production bundles. */
export const CATALOGUE_MARKER = 'KASPALIONARE_DEV_CATALOGUE';

const NOW = new Date(2026, 9, 25, 12, 0, 0).getTime();
const VDAA = '13000';
const COV_TX = '9f3c0a11223344556677889900aabbccddeeff00112233445566778899b71e21';
const ADDR = 'kaspatest:qqdasdj3abcdefghijklmnopqrstuvwxyz0123456789abcdefghijphyfvu';

function Row({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2 border-t border-rule-soft pt-3">
      <span className="font-mono text-[12px] text-graphite">{name}</span>
      <div className="flex flex-wrap items-center gap-2">{children}</div>
    </div>
  );
}

function Sheet() {
  const balls: NumberBallState[] = ['free', 'picked', 'matched', 'full', 'disabled'];
  const tiles: BonusTileState[] = ['free', 'picked', 'matched'];
  return (
    <div className="flex flex-col gap-4">
      <Row name="Number ball: free · picked · matched · full · disabled">
        {balls.map((s, i) => (
          <NumberBall key={s} n={[8, 11, 17, 21, 30][i] ?? 1} state={s} />
        ))}
      </Row>
      <Row name="Bonus tile: free · picked · matched">
        {tiles.map((s, i) => (
          <BonusTile key={s} n={i + 1} state={s} />
        ))}
      </Row>
      <Row name="Round status pill">
        <RoundStatusPill status="open" />
        <RoundStatusPill status="closing" />
        <RoundStatusPill status="drawn" />
        <RoundStatusPill status="void" />
      </Row>
      <Row name="Amount (tap: exact sompi)">
        <Amount sompi="4820000000000" />
        <Amount sompi="13000000" />
        <Amount sompi="32254200" />
      </Row>
      <Row name="Time (clock and DAA, 10 per second)">
        <Time daa="13662" virtualDaa={VDAA} observedAt={NOW} now={NOW} />
        <Time daa="6100000" virtualDaa={VDAA} observedAt={NOW} now={NOW} />
      </Row>
      <Row name="Hash or address">
        <HashOrAddress value={COV_TX} kind="transaction" explorer="https://explorer-tn10.kaspa.org" testId="cat-tx" />
        <HashOrAddress value={ADDR} kind="address" testId="cat-addr" />
      </Row>
      <Row name="Jackpot and countdown: live · deadline passed · stale">
        <div className="w-full">
          <JackpotCountdown jackpot="4820000000000" deadline="13662" virtualDaa={VDAA} observedAt={NOW} now={NOW} />
        </div>
        <div className="w-full">
          <JackpotCountdown
            jackpot="4820000000000"
            deadline="12000"
            virtualDaa={VDAA}
            observedAt={NOW}
            now={NOW}
            runHref="#/rounds"
          />
        </div>
        <div className="w-full">
          <JackpotCountdown
            jackpot="4820000000000"
            deadline="13662"
            virtualDaa={VDAA}
            observedAt={NOW - 60_000}
            now={NOW}
            stale
          />
        </div>
      </Row>
      <Row name="Transaction progress: built · signed · sent · accepted · failed · rebuilt">
        <TxProgress step="built" />
        <TxProgress step="signed" />
        <TxProgress step="sent" />
        <TxProgress step="accepted" />
        <TxProgress step="signed" failed="The network node refused the request." />
        <TxProgress step="built" rebuilt={{ tries: 1, max: 3 }} />
      </Row>
      <Row name="Ticket builder row: combination · remove">
        <ul className="m-0 w-full list-none p-0">
          <TicketRow index={0} main={[3, 7, 12, 19, 28]} bonus={2} onRemove={() => undefined} />
          <TicketRow index={1} main={[3, 7, 12, 19, 28]} bonus={2} />
        </ul>
      </Row>
      <Row name="Cost panel: empty · estimating · ready · fee too high">
        {(['empty', 'estimating', 'ready', 'fee-too-high'] as const).map((s) => (
          <div key={s} className="w-full">
            <CostPanel
              state={s}
              rows={[
                { id: 'price', name: 'Price', sompi: '200000000' },
                { id: 'deposit', name: 'Deposit', sompi: '26000000', note: 'back 2 Nov' },
                { id: 'fee', name: 'Network fee', sompi: '32254200' },
                { id: 'referral', name: 'Referral', sompi: '20000000', note: 'qqdasd…phyfvu' },
              ]}
              total="258254200"
              warning={s === 'fee-too-high' ? 'The fee is larger than the prize.' : undefined}
            />
          </div>
        ))}
      </Row>
      <Row name="Round card: loading · error · ready">
        <div className="w-full">
          <RoundCardView load={{ kind: 'loading' }} onRetry={() => undefined} />
        </div>
        <div className="w-full">
          <RoundCardView
            load={{ kind: 'error', message: 'The indexer cannot be reached.', code: 'indexer_unavailable', detail: '' }}
            onRetry={() => undefined}
          />
        </div>
        <div className="w-full">
          <RoundCardView
            now={NOW}
            load={{
              kind: 'ready',
              stale: false,
              data: {
                round: '12',
                status: 'open',
                statusChecked: true,
                m: 10,
                jackpot: '4820000000000',
                deadline: '13662',
                odds: '1425060',
                virtualDaaScore: VDAA,
                observedAt: NOW,
                node: {
                  url: 'ws://127.0.0.1:18610',
                  network: 'simnet',
                  serverVersion: '2.1.0',
                  isSynced: true,
                  virtualDaaScore: VDAA,
                },
              },
            }}
            onRetry={() => undefined}
          />
        </div>
      </Row>
    </div>
  );
}

export default function Catalogue() {
  return (
    <section
      data-testid="catalogue"
      data-marker={CATALOGUE_MARKER}
      aria-labelledby="catalogue-title"
      className="flex flex-col gap-4"
    >
      <h1 id="catalogue-title" className="m-0 text-[17px] font-semibold">
        Components
      </h1>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div
          data-theme="dark"
          data-testid="catalogue-dark"
          className="rounded-[14px] border border-rule bg-ground p-3 text-ink"
        >
          <Sheet />
        </div>
        <div
          data-theme="light"
          data-testid="catalogue-light"
          className="rounded-[14px] border border-rule bg-ground p-3 text-ink"
        >
          <Sheet />
        </div>
      </div>
    </section>
  );
}
