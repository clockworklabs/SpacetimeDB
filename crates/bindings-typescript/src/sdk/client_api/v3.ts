/* eslint-disable */
/* tslint:disable */
import { t as __t, type Infer as __Infer } from '../../lib/type_builders';

export const ClientFrame = __t.enum('ClientFrame', {
  Single: __t.byteArray(),
  Batch: __t.array(__t.byteArray()),
});
export type ClientFrame = __Infer<typeof ClientFrame>;

export const ServerFrame = __t.enum('ServerFrame', {
  Single: __t.byteArray(),
  Batch: __t.array(__t.byteArray()),
});
export type ServerFrame = __Infer<typeof ServerFrame>;
