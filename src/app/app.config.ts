import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core';
import { provideRouter } from '@angular/router';
import { provideNexusHost } from '@bimo-dk/nexus-runtime';
import { routes } from './app.routes';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    // Provides NEXUS_CONFIG + HttpClient + DynamicNexusService + HealthService.
    // The host shell runs in this injector, so all nexus-runtime services must be
    // provided here. Token is loaded at container start from /assets/config.json.
    provideNexusHost({
      configDefaults: {
        registryUrl: '/api',
        nexusToken: '',
        staticBackupUrl: '/assets/registry-backup/remotes.json',
      },
    }),
  ],
};
