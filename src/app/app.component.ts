import { ChangeDetectionStrategy, Component, Type, inject, signal } from '@angular/core';
import { NgComponentOutlet } from '@angular/common';
import { HostLoaderService } from './services/host-loader.service';

@Component({
  selector: 'app-root',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgComponentOutlet],
  template: `
    @if (hostComponent()) {
      <ng-container *ngComponentOutlet="hostComponent()!" />
    } @else if (loader.status() === 'loading') {
      <div class="splash">
        <div class="spinner"></div>
        <h1>Loading host shell...</h1>
        <p>Attempt {{ loader.attempt() }} of 3</p>
      </div>
    } @else if (loader.status() === 'failed') {
      <div class="splash error">
        <h1>Host shell unavailable</h1>
        <p>Failed to load host after 3 attempts.</p>
        <pre>{{ loader.lastError() }}</pre>
        <button type="button" (click)="retry()">Try again</button>
      </div>
    }
  `,
  styles: [
    `
      .splash {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100vh;
        text-align: center;
        padding: 24px;
        color: var(--app-text);
      }
      .splash.error { color: #991b1b; }
      .splash h1 { margin: 12px 0 4px; font-size: 20px; }
      .splash p { margin: 0; color: var(--app-text-muted); }
      .splash pre {
        background: #fee2e2;
        padding: 12px 16px;
        border-radius: 8px;
        max-width: 600px;
        overflow: auto;
        margin: 16px 0;
        text-align: left;
        font-size: 12px;
      }
      .splash button {
        background: var(--app-primary);
        color: white;
        border: none;
        padding: 10px 20px;
        border-radius: 6px;
        font-weight: 600;
        cursor: pointer;
      }
      .splash button:hover { background: #4f46e5; }
      .spinner {
        width: 40px;
        height: 40px;
        border: 4px solid #e2e8f0;
        border-top-color: var(--app-primary);
        border-radius: 999px;
        animation: spin 0.8s linear infinite;
      }
      @keyframes spin { to { transform: rotate(360deg); } }
    `,
  ],
})
export class AppComponent {
  readonly loader = inject(HostLoaderService);
  readonly hostComponent = signal<Type<unknown> | null>(null);

  constructor() {
    this.load();
  }

  async retry(): Promise<void> {
    await this.load();
  }

  private async load(): Promise<void> {
    try {
      const c = await this.loader.loadShell();
      this.hostComponent.set(c);
    } catch (err) {
      console.error('[app] Could not bootstrap host shell:', err);
    }
  }
}
