import { mount } from 'svelte';
import Root from './Root.svelte';

mount(Root, {
  target: document.getElementById('app')!,
});
