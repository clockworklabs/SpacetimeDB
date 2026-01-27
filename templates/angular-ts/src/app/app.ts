import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { injectSpacetimeDBConnected } from 'spacetimedb/angular';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet],
  template: `
    @if (isConnected()) {
      <router-outlet />
    } @else {
      <div class="loading">
        <p>Connecting to SpacetimeDB...</p>
      </div>
    }
  `,
})
export class App {
  protected isConnected = injectSpacetimeDBConnected();
}
