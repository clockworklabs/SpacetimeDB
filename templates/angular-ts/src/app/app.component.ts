import { Component } from '@angular/core';
import {
  injectSpacetimeDB,
  injectTable,
  injectReducer,
} from 'spacetimedb/angular';
import { tables, reducers } from '../module_bindings';

@Component({
  selector: 'app-root',
  template: `
    <div style="padding: 2rem">
      <h1>SpacetimeDB Angular App</h1>

      <div style="margin-bottom: 1rem">
        Status:
        <strong [style.color]="conn().isActive ? 'green' : 'red'">
          {{ conn().isActive ? 'Connected' : 'Disconnected' }}
        </strong>
      </div>

      <form (submit)="addPerson($event)" style="margin-bottom: 2rem">
        <input
          type="text"
          placeholder="Enter name"
          [value]="name"
          (input)="name = $any($event.target).value"
          style="padding: 0.5rem; margin-right: 0.5rem"
          [disabled]="!conn().isActive"
        />
        <button
          type="submit"
          style="padding: 0.5rem 1rem"
          [disabled]="!conn().isActive"
        >
          Add Person
        </button>
      </form>

      <div>
        <h2>People ({{ people().rows.length }})</h2>
        @if (people().rows.length === 0) {
          <p>No people yet. Add someone above!</p>
        } @else {
          <ul>
            @for (person of people().rows; track $index) {
              <li>{{ person.name }}</li>
            }
          </ul>
        }
      </div>
    </div>
  `,
})
export class App {
  protected conn = injectSpacetimeDB();
  protected people = injectTable(tables.person);
  private addReducer = injectReducer(reducers.add);
  protected name = '';

  addPerson(event: Event) {
    event.preventDefault();
    if (!this.name.trim() || !this.conn().isActive) return;

    // Call the add reducer
    this.addReducer({ name: this.name });
    this.name = '';
  }
}
