import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core';
import { provideRouter } from '@angular/router';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { provideNexusConfig } from '@bimo-dk/nexus-runtime';
import { routes } from './app.routes';
import { nexusAuthInterceptor } from './interceptors/nexus-auth.interceptor';
import { correlationIdInterceptor } from './interceptors/correlation-id.interceptor';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    // Both interceptors registered here — effective for all Angular services including
    // the host shell loaded via federation, which runs in this application's injector.
    provideHttpClient(withInterceptors([nexusAuthInterceptor, correlationIdInterceptor])),
    // Provides NEXUS_CONFIG so host-shell services (DynamicNexusService etc.) can inject it.
    // The host shell runs in this injector context, not its own bootstrap scope.
    provideNexusConfig({
      registryUrl: '/api',
      nexusToken: 'dev-token-change-in-production',
      staticBackupUrl: '/assets/registry-backup/remotes.json',
    }),
  ],
};
