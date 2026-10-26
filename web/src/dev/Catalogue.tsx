// The component catalogue (spec 1.1 DX): every base component in every state, in both themes side by
// side. Development builds only (#/dev/catalogue; App.tsx imports this file behind import.meta.env.DEV,
// and scripts/check-bundle.mjs checks that a production bundle has no trace of it). The values are
// sample figures for the catalogue only.
import type { ReactNode } from 'react';
import { Amount } from '@/components/Amount';
import { BallRow } from '@/components/BallRow';
import { BonusTile, type BonusTileState } from '@/components/BonusTile';
import { ClaimRow } from '@/components/ClaimRow';
import { LocalCopyBox } from '@/components/LocalCopyBox';
import { PendingPurchase } from '@/components/PendingPurchase';
import { TicketCard } from '@/components/TicketCard';
import { CostPanel } from '@/components/CostPanel';
import { HashOrAddress } from '@/components/HashOrAddress';
import { JackpotCountdown } from '@/components/JackpotCountdown';
import { NumberBall, type NumberBallState } from '@/components/NumberBall';
import { RoundStatusPill } from '@/components/RoundStatusPill';
import { Time } from '@/components/Time';
import { TicketRow } from '@/components/TicketRow';
import { TxProgress } from '@/components/TxProgress';
import { RoundCardView } from '@/screens/play/RoundCard';
import { CallerPanel } from '@/components/CallerPanel';
import { RoundRow } from '@/components/RoundRow';
import { VerifyPanel } from '@/components/VerifyPanel';
import { SAMPLE_CLOSE, SAMPLE_DRAW_LOW, SAMPLE_VERIFY } from './roundSamples';
import { claimView, SAMPLE_TX, sampleCard, sampleFigures, samplePending, sampleStored } from './ticketSamples';

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
      <Row name="Ball row: drawn, four main numbers and the bonus matched">
        <BallRow
          combination={{ main: [3, 7, 12, 19, 28], bonus: 2 }}
          matchedMain={[true, true, false, true, true]}
          bonusMatched
          testId="cat-balls"
        />
      </Row>
      <Row name="Claim row: claimable · claiming · claimed · accrued below P_MIN · fee exceeds prize">
        <div className="flex w-full flex-col gap-2">
          <ClaimRow
            testId="cat-claim-claimable"
            claim={claimView('pay', '1240000000', '2')}
            figures={sampleFigures(claimView('pay', '1240000000', '2'), '25019700')}
            deadline="6100000"
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
          />
          <ClaimRow
            testId="cat-claim-claiming"
            claim={claimView('final', '1240000000')}
            figures={sampleFigures(claimView('final', '1240000000'), '6172000', '26000000')}
            tx={{ step: 'signed' }}
          />
          <ClaimRow testId="cat-claim-claimed" claimed />
          <ClaimRow
            testId="cat-claim-accrued"
            claim={claimView('accrue', '2240000')}
            figures={sampleFigures(claimView('accrue', '1000'), '1000')}
          />
          <ClaimRow
            testId="cat-claim-fee"
            claim={claimView('final', '2240000')}
            figures={sampleFigures(claimView('final', '2240000'), '14474100', '26000000')}
          />
        </div>
      </Row>
      <Row name="Ticket card: open · win · no win · partly claimed · fully claimed · void · deposit returnable · deposit returned · incomplete · expired and no pool · another payout">
        <div className="flex w-full flex-col gap-2">
          {(['open', 'win', 'no-win', 'partly', 'void', 'returnable', 'incomplete', 'expired'] as const).map((k) => (
            <TicketCard
              key={k}
              testId={`cat-ticket-${k}`}
              stored={sampleStored('12')}
              status={k === 'open' ? 'open' : k === 'void' ? 'void' : 'drawn'}
              card={sampleCard(k)}
              clock={{ virtualDaa: VDAA, observedAt: NOW }}
              claims={{ 0: { figures: sampleFigures(claimView('pay', '1240000000', '2'), '25019700') } }}
              ret={{
                figures: {
                  round: '12',
                  path: 'alone',
                  amount: '26000000',
                  payout: ADDR,
                  fee: '5166400',
                  lockTime: '12109662',
                },
              }}
            />
          ))}
          <TicketCard
            testId="cat-ticket-payout"
            stored={sampleStored('12')}
            status="drawn"
            card={sampleCard('win')}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            claims={{ 0: { figures: sampleFigures(claimView('pay', '1240000000', '2'), '25019700') } }}
            wallet="kaspatest:qr7wallet0another0address000000000000000000000000000000000"
          />
          <TicketCard
            testId="cat-ticket-claimed"
            stored={sampleStored('11', {
              kind: 'claimed',
              transactionId: SAMPLE_TX,
              amount: '1266000000',
              endedMs: '1',
            })}
            status="drawn"
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
          />
          <TicketCard
            testId="cat-ticket-returned"
            stored={sampleStored('10', {
              kind: 'returned',
              transactionId: SAMPLE_TX,
              amount: '26000000',
              endedMs: '1',
            })}
            status="drawn"
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
          />
        </div>
      </Row>
      <Row name="Pending purchase: unknown · sending · failed">
        <div className="flex w-full flex-col gap-2">
          {(['unknown', 'sending', 'failed'] as const).map((s) => (
            <PendingPurchase
              key={s}
              testId={`cat-pending-${s}`}
              pending={samplePending()}
              state={s}
              error={s === 'failed' ? 'The node could not be reached.' : undefined}
              onSend={() => undefined}
            />
          ))}
        </div>
      </Row>
      <Row name="Local-copy box: complete and verified · incomplete · mismatch · not needed">
        <div className="flex w-full flex-col gap-2">
          {(['complete', 'incomplete', 'mismatch', 'not-needed'] as const).map((s) => (
            <LocalCopyBox
              key={s}
              state={s}
              rounds={s === 'not-needed' ? 0 : 2}
              tickets={s === 'not-needed' ? 0 : 3}
              onDownload={() => undefined}
            />
          ))}
        </div>
      </Row>
      <Row name="Round row: checking · drawn · open · unverified · error">
        <div className="flex w-full flex-col gap-2">
          <RoundRow round="14" testId="cat-row-checking" />
          <RoundRow
            round="13"
            testId="cat-row-drawn"
            data={{
              round: '13',
              status: 'drawn',
              sales: { value: '48200000000', checked: false },
              jackpot: { value: '4820000000000', checked: false },
            }}
          />
          <RoundRow
            round="12"
            testId="cat-row-open"
            data={{
              round: '12',
              status: 'open',
              sales: { value: '48200000000', checked: true },
              jackpot: { value: '4820000000000', checked: true },
            }}
          />
          <RoundRow
            round="11"
            testId="cat-row-unverified"
            data={{ round: '11', status: null, sales: { value: '0', checked: false } }}
          />
          <RoundRow
            round="10"
            testId="cat-row-error"
            data={{
              round: '10',
              status: null,
              error: "The indexer's copy does not match the chain.",
              code: 'chain_mismatch',
            }}
          />
        </div>
      </Row>
      <Row name="Verify: idle · running · done (match, mismatch, not checkable) · error">
        <div className="flex w-full flex-col gap-2">
          <VerifyPanel state="idle" onVerify={() => undefined} testId="cat-verify-idle" />
          <VerifyPanel state="running" onVerify={() => undefined} testId="cat-verify-running" />
          <VerifyPanel state="done" result={SAMPLE_VERIFY} onVerify={() => undefined} testId="cat-verify-done" />
          <VerifyPanel
            state="error"
            error="The indexer cannot be reached."
            onVerify={() => undefined}
            testId="cat-verify-error"
          />
        </div>
      </Row>
      <Row name="Caller panel: not due · waits · due with reward · reward below fee · running">
        <div className="flex w-full flex-col gap-2">
          <CallerPanel
            round="12"
            due={{ kind: 'later', action: 'close', from: '13662' }}
            reward="2410000000"
            walletConnected
            onPress={() => undefined}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            testId="cat-caller-later"
          />
          <CallerPanel
            round="12"
            due={{ kind: 'waits' }}
            walletConnected
            onPress={() => undefined}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            testId="cat-caller-waits"
          />
          <CallerPanel
            round="12"
            due={{ kind: 'due', action: 'close', from: '12000' }}
            figures={SAMPLE_CLOSE}
            walletConnected
            onPress={() => undefined}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            testId="cat-caller-due"
          />
          <CallerPanel
            round="12"
            due={{ kind: 'due', action: 'draw', from: '12000' }}
            figures={SAMPLE_DRAW_LOW}
            walletConnected
            onPress={() => undefined}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            testId="cat-caller-low"
          />
          <CallerPanel
            round="12"
            due={{ kind: 'due', action: 'void', from: '12000' }}
            figures={{ ...SAMPLE_CLOSE, kind: 'void' }}
            tx={{ step: 'signed' }}
            walletConnected
            onPress={() => undefined}
            clock={{ virtualDaa: VDAA, observedAt: NOW }}
            testId="cat-caller-running"
          />
        </div>
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
