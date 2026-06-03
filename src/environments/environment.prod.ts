// Runtime-overridable config. These values can be overridden via /assets/config.json
// generated at container start from env-vars (see Dockerfile + docker-entrypoint.d/40-runtime-config.sh).
// Sane defaults are used if no runtime config is found (e.g. standalone build/test).
export const environment = {
  production: true,
  hostRemoteEntry: '/host/remoteEntry.json',
  hostExposedModule: './AppShell',
  retryAttempts: 3,
  retryDelayMs: 2000,
};
