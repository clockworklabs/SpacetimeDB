import type { ProductType } from '../lib/algebraic_type';
import type { ParamsObj } from '../lib/reducers';
import type { CoerceRow } from '../lib/table';
import type { InferTypeOfRow } from '../lib/type_builders';
import type { CamelCase, PascalCase } from '../lib/type_util';
import type { CallReducerFlags } from './db_connection_impl';
import type { UntypedRemoteModule } from './spacetime_module';
import type {
  ReducerEventContextInterface,
  SubscriptionEventContextInterface,
} from './event_context';

export type ReducerEventCallback<
  RemoteModule extends UntypedRemoteModule,
  ReducerArgs extends object = object,
> = (
  ctx: ReducerEventContextInterface<RemoteModule>,
  args: ReducerArgs
) => void;

export type SubscriptionEventCallback<
  RemoteModule extends UntypedRemoteModule,
> = (ctx: SubscriptionEventContextInterface<RemoteModule>) => void;

// Utility: detect 'any'
type IfAny<T, Y, N> = 0 extends 1 & T ? Y : N;

// Loose shape that allows all three families even when key names are unknown
type ReducersViewLoose = {
  // call: camelCase(name)
  [k: string]: (params: any) => void;
} & {
  // onX
  [k: `on${string}`]: (callback: ReducerEventCallback<any, any>) => void;
} & {
  // removeOnX
  [k: `removeOn${string}`]: (callback: ReducerEventCallback<any, any>) => void;
};

export type ReducersView<RemoteModule> = IfAny<
  RemoteModule,
  ReducersViewLoose,
  RemoteModule extends UntypedRemoteModule
    ? // x: camelCase(name)
      {
        [K in RemoteModule['reducers'][number] as CamelCase<
          K['accessorName']
        >]: (params: InferTypeOfRow<K['params']>) => void;
      } & // onX: `on${PascalCase(name)}`
      {
        [K in RemoteModule['reducers'][number] as `on${PascalCase<K['accessorName']>}`]: (
          callback: ReducerEventCallback<
            RemoteModule,
            InferTypeOfRow<K['params']>
          >
        ) => void;
      } & // removeOnX: `removeOn${PascalCase(name)}`
      {
        [K in RemoteModule['reducers'][number] as `removeOn${PascalCase<K['accessorName']>}`]: (
          callback: ReducerEventCallback<
            RemoteModule,
            InferTypeOfRow<K['params']>
          >
        ) => void;
      }
    : never
>;

export type ReducerEventInfo<
  Name extends string = string,
  Args extends object = object,
> = {
  name: Name;
  args: Args;
};

export type UntypedReducerDef = {
  name: string;
  accessorName: string;
  params: CoerceRow<ParamsObj>;
  paramsType: ProductType;
};

export type UntypedReducersDef = {
  reducers: readonly UntypedReducerDef[];
};

export type SetReducerFlags<R extends UntypedReducersDef> = {
  [K in keyof R['reducers'] as CamelCase<R['reducers'][number]['name']>]: (
    flags: CallReducerFlags
  ) => void;
};
