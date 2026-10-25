import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App, ConfigProblems } from '@/app/App';
import { createCtx } from '@/app/ctx';
import { CONFIG } from '@/config';
import './index.css';

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      {'config' in CONFIG ? <App ctx={createCtx(CONFIG.config)} /> : <ConfigProblems problems={CONFIG.problems} />}
    </StrictMode>,
  );
}
