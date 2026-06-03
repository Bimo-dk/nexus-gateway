import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core';
import { provideRouter } from '@angular/router';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { routes } from './app.routes';
import { nexusAuthInterceptor } from './interceptors/nexus-auth.interceptor';
import { correlationIdInterceptor } from './interceptors/correlation-id.interceptor';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection({ eventCoalescing: true }),
    provideRouter(routes),
    // Bemærk: app's appConfig er den effektive config FOR ALLE Angular-services der
    // kører i denne browser-instans (inkl. host loaded via federation). Derfor
    // skal BÅDE nexusAuth + correlationId interceptors registreres her,
    // ikke kun i host/manager — selvom de også har egne kopier til standalone-brug.
    provideHttpClient(withInterceptors([nexusAuthInterceptor, correlationIdInterceptor])),
  ],
};
