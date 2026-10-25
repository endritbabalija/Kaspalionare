// The wallet session's shape and context (wallet/session.tsx provides it).
import { createContext, useContext } from 'react';
import type { WalletAdapter, WalletKind } from './types';

export type WalletState = 'disconnected' | 'connecting' | 'wrong-network' | 'connected' | 'quick';

export interface WalletSession {
  state: WalletState;
  kind?: WalletKind;
  address?: string;
  adapter?: WalletAdapter;
  /** The last failure's code and message. */
  error?: { code: string | null; message: string };
  /** Whether this build has the quick wallet. */
  quickAvailable: boolean;
  /** Whether this browser holds a quick wallet key. */
  quickStored: boolean;
  connect(kind: 'kasware' | 'kaspire'): Promise<void>;
  openQuick(): Promise<void>;
  createQuick(): Promise<void>;
  importQuick(secretKey: string): Promise<void>;
  /** The quick wallet's secret key, for its export (spec S7). */
  exportQuick(): Promise<string>;
  disconnect(): void;
}

export const WalletContext = createContext<WalletSession | null>(null);

export function useWallet(): WalletSession {
  const w = useContext(WalletContext);
  if (!w) throw new Error('useWallet outside WalletProvider');
  return w;
}
