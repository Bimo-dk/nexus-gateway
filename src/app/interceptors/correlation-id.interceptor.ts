import { HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { tap } from 'rxjs';

const SESSION_ID = generateSessionId();
let requestCounter = 0;

function generateSessionId(): string {
  const cryptoObj = globalThis.crypto;
  if (cryptoObj && typeof cryptoObj.randomUUID === 'function') {
    return cryptoObj.randomUUID().substring(0, 8);
  }
  return Math.random().toString(36).substring(2, 10);
}

export const correlationIdInterceptor: HttpInterceptorFn = (req, next) => {
  requestCounter++;
  const correlationId = `${SESSION_ID}-${requestCounter}`;
  const withHeader = req.clone({
    setHeaders: { 'X-Request-ID': correlationId },
  });

  return next(withHeader).pipe(
    tap({
      error: (err: unknown) => {
        if (err instanceof HttpErrorResponse) {
          console.error(
            `[http] FAILED ${req.method} ${req.url} → ${err.status} (X-Request-ID=${correlationId})`,
            err.error,
          );
        }
      },
    }),
  );
};
