import type {
  OptionBuilder,
  StringBuilder,
  TypeBuilder,
  t,
} from './type_builders';

/** Only strings and simple, payload-free enums constrain environment strings. */
export type EnvironmentString =
  | StringBuilder
  | (TypeBuilder<any, any> & {
      readonly variants: Record<string, ReturnType<typeof t.unit>>;
    });
export type EnvironmentSchema = Record<
  string,
  EnvironmentString | OptionBuilder<EnvironmentString>
>;

export type EnvironmentValue<T> =
  T extends OptionBuilder<infer Inner>
    ? EnvironmentValue<Inner> | undefined
    : T extends { readonly variants: infer Variants }
      ? keyof Variants & string
      : string;

/** Values are read from the host on each access. Undeclared names are errors. */
export type Environment<
  Declarations extends EnvironmentSchema | undefined = undefined,
> = Declarations extends EnvironmentSchema
  ? {
      readonly [Key in keyof Declarations as Key extends 'get'
        ? never
        : Key]: EnvironmentValue<Declarations[Key]>;
    } & {
      get<const Key extends string>(
        key: Key &
          (string extends Key
            ? unknown
            : Key extends keyof Declarations
              ? unknown
              : never)
      ): Key extends keyof Declarations
        ?
            | Exclude<EnvironmentValue<Declarations[Key]>, undefined>
            | (undefined extends EnvironmentValue<Declarations[Key]>
                ? null
                : never)
        : string | null;
    }
  : { get(key: string): string | null };

export type EnvironmentFor<Schema> = Schema extends {
  env: infer Declarations extends EnvironmentSchema;
}
  ? Environment<Declarations>
  : Environment;
