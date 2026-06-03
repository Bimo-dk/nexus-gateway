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
    // Note: app's appConfig is the effective config FOR ALL Angular services that
    // run in this browser instance (incl. host loaded via federation). Therefore
    // BOTH nexusAuth + correlationId interceptors must be registered here,
    // not only in host/manager — even though they also have their own copies for standalone use.
    provideHttpClient(withInterceptors([nexusAuthInterceptor, correlationIdInterceptor])),
  ],
};
