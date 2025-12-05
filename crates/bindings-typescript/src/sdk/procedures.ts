import type { Infer, InferTypeOfRow } from '../lib/type_builders';
import type { CamelCase } from '../lib/type_util';
import type { UntypedRemoteModule } from './spacetime_module';

// Utility: detect 'any'
type IfAny<T, Y, N> = 0 extends 1 & T ? Y : N;

// Loose shape that allows all three families even when key names are unknown
type ProceduresViewLoose = {
  // call: camelCase(name)
  [k: string]: (params: any) => Promise<any>;
};

export type ProceduresView<RemoteModule> = IfAny<
  RemoteModule,
  ProceduresViewLoose,
  RemoteModule extends UntypedRemoteModule
    ? // x: camelCase(name)
      {
        [K in RemoteModule['procedures'][number] as CamelCase<
          K['accessorName']
        >]: (
          params: InferTypeOfRow<K['params']>
        ) => Promise<Infer<K['returnType']>>;
      }
    : never
>;
