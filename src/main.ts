import { initFederation } from '@angular-architects/native-federation';

initFederation()
  .catch((err) => console.error('[app] Federation init failed:', err))
  .then(() => import('./bootstrap'))
  .catch((err) => console.error('[app] Bootstrap failed:', err));
