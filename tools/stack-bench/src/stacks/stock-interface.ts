// The warehouse contract names the stock tables other systems write directly.
// A direct write that cannot find them is the application not providing that
// interface, not a harness fault; the marker lets the grader grade it so.
export function stockInterfaceError(message: string, options: { cause?: unknown } = {}): Error {
  return Object.assign(new Error(message, options), { stockInterface: true });
}
