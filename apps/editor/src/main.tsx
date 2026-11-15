// Entry point: load the runtime, open a document, install shortcuts, mount the app.

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.tsx';
import './styles.css';
import { defaultDocument } from './doc/ops.ts';
import { openExample } from './io/files.ts';
import { initRuntime } from './runtime.ts';
import { installShortcuts } from './shortcuts.ts';
import { store } from './store.ts';

const root = document.getElementById('root')!;

try {
  await initRuntime();
  store.open(defaultDocument(), 'Untitled.json', null);
  // Dev hook: `?example=button` opens examples/button.json.
  const example = new URLSearchParams(location.search).get('example');
  if (example) await openExample(example);
  installShortcuts();
  // Leaving with unsaved changes asks first.
  window.addEventListener('beforeunload', (e) => {
    if (store.get().doc !== store.get().saved) e.preventDefault();
  });
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
} catch (e) {
  root.textContent = `Zuri could not start: ${e instanceof Error ? e.message : String(e)}`;
}
