import type { UntypedRemoteModule } from './spacetime_module.ts';
import type { ClientTable } from './client_table.ts';
import type { NestAccessors, Values } from '../lib/type_util.ts';

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
type IfAny<T, Y, N> = 0 extends 1 & T ? Y : N;

type ClientDbViewLoose = { readonly [k: string]: any };

export type ClientDbView<RemoteModule extends UntypedRemoteModule> = IfAny<
  RemoteModule,
  ClientDbViewLoose,
  NestAccessors<{
    readonly [TblName in Values<
      RemoteModule['tables']
    >['accessorName']]: ClientTable<RemoteModule, TblName>;
  }>
>;
