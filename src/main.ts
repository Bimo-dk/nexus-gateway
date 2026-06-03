import { initFederation } from '@angular-architects/native-federation';
import { environment } from './environments/environment';

/**
 * Bootstrap-sekvens med runtime-config override.
 *
 * 1. Fetch /assets/config.json (genereret af nginx entrypoint fra container env-vars)
 * 2. Merge ind i environment-objektet → overrider compile-time defaults
 * 3. Init federation runtime
 * 4. Bootstrap Angular
 *
 * Hvis config.json mangler (fx standalone dev-build), beholdes default-værdier.
 */
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
