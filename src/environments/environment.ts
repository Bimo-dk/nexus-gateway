export const environment = {
  production: false,
  // Relativ — fetches via app's egen nginx der proxy'er /host/ til host:80
  hostRemoteEntry: '/host/remoteEntry.json',
  hostExposedModule: './AppShell',
  retryAttempts: 3,
  retryDelayMs: 2000,
};
