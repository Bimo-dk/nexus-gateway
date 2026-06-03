import { initFederation } from '@angular-architects/native-federation';
import { environment } from './environments/environment';

// Capture the URL the user originally requested. Angular's Router (with empty
// gateway routes) may rewind window.location to '/' when no route matches the
// initial navigation, before the federated host shell can register its routes.
// Stashing it on globalThis lets the host pick it up in its ngOnInit.
declare global { interface Window { __nexusInitialUrl?: string } }
window.__nexusInitialUrl = window.location.pathname + window.location.search + window.location.hash;

async function start(): Promise<void> {
  try {
    const res = await fetch('/assets/config.json', { cache: 'no-store' });
    if (res.ok) {
      const runtime = (await res.json()) as Partial<typeof environment>;
      Object.assign(environment, runtime);
      console.log('[gateway] Runtime config loaded:', runtime);
    }
  } catch (err) {
    console.warn('[gateway] No runtime config — using compile-time defaults', err);
  }

  try {
    await initFederation();
    await import('./bootstrap');
  } catch (err) {
    console.error('[gateway] Bootstrap failed:', err);
  }
}

start();
