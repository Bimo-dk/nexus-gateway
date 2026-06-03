import { HttpInterceptorFn } from '@angular/common/http';

// App's Nexus token — bagt ind ved Docker build
const NEXUS_TOKEN = 'NEXUS_TOKEN_PLACEHOLDER';

export const nexusAuthInterceptor: HttpInterceptorFn = (req, next) => {
  // Tilføj X-Nexus-Token når host (kørende i app's runtime) kalder /api/...
  if (!req.url.startsWith('/api/') && !req.url.includes('/api/remotes')) {
    return next(req);
  }
  const authed = req.clone({
    setHeaders: { 'X-Nexus-Token': NEXUS_TOKEN },
  });
  return next(authed);
};
