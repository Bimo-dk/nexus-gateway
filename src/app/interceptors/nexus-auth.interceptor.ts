import { HttpInterceptorFn } from '@angular/common/http';

// App's Nexus token — baked in at Docker build time
const NEXUS_TOKEN = 'NEXUS_TOKEN_PLACEHOLDER';

export const nexusAuthInterceptor: HttpInterceptorFn = (req, next) => {
  // Add X-Nexus-Token when host (running in app's runtime) calls /api/...
  if (!req.url.startsWith('/api/') && !req.url.includes('/api/remotes')) {
    return next(req);
  }
  const authed = req.clone({
    setHeaders: { 'X-Nexus-Token': NEXUS_TOKEN },
  });
  return next(authed);
};
