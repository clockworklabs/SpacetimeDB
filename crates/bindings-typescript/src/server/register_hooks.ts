import { register_hooks } from 'spacetime:sys@1.0';
import { register_hooks as register_hooks_v1_1 } from 'spacetime:sys@1.1';
import { register_hooks as register_hooks_v1_2 } from 'spacetime:sys@1.2';
import { hooks, hooks_v1_1, hooks_v1_2 } from './runtime';

register_hooks(hooks);
register_hooks_v1_1(hooks_v1_1);
register_hooks_v1_2(hooks_v1_2);
