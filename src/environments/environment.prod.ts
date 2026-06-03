// Runtime-overridable config. Disse værdier kan overrides via /assets/config.json
// der genereres ved container-start fra env-vars (se Dockerfile + docker-entrypoint.d/40-runtime-config.sh).
// Sane defaults bruges hvis ingen runtime config findes (fx ved standalone build/test).
export const environment = {
  production: true,
  hostRemoteEntry: '/host/remoteEntry.json',
  hostExposedModule: './AppShell',
  retryAttempts: 3,
  retryDelayMs: 2000,
};
