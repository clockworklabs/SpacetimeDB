// Ambient declaration for the host syscall used by the failure-reporting
// bridge. The SDK keeps this module internal, while the module bundler
// resolves the direct import inside the host. Keep this dependency isolated
// here so it can be replaced without changing the public cron API.
declare module 'spacetime:sys@2.0' {
  export function volatile_nonatomic_schedule_immediate(
    reducer_name: string,
    args: Uint8Array
  ): void;
}
