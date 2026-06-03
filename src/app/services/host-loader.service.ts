import { Injectable, Type, signal } from '@angular/core';
import { loadRemoteModule } from '@angular-architects/native-federation';
import { environment } from '../../environments/environment';

@Injectable({ providedIn: 'root' })
export class HostLoaderService {
  readonly status = signal<'idle' | 'loading' | 'ready' | 'failed'>('idle');
  readonly lastError = signal<string | null>(null);
  readonly attempt = signal<number>(0);

  async loadShell(): Promise<Type<unknown>> {
    this.status.set('loading');
    this.lastError.set(null);

    for (let i = 1; i <= environment.retryAttempts; i++) {
      this.attempt.set(i);
      try {
        const moduleRef = await loadRemoteModule({
          remoteEntry: environment.hostRemoteEntry,
          exposedModule: environment.hostExposedModule,
        });
        const component = moduleRef.AppComponent ?? moduleRef.default ?? moduleRef[Object.keys(moduleRef)[0]];
        if (!component) {
          throw new Error('Host did not expose a component on ./AppShell');
        }
        this.status.set('ready');
        console.log(`[app] Host shell loaded on attempt ${i}`);
        return component as Type<unknown>;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        this.lastError.set(msg);
        console.warn(`[app] Host load attempt ${i}/${environment.retryAttempts} failed: ${msg}`);
        if (i < environment.retryAttempts) {
          await this.delay(environment.retryDelayMs);
        }
      }
    }

    this.status.set('failed');
    throw new Error(`Failed to load host shell after ${environment.retryAttempts} attempts: ${this.lastError()}`);
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}
