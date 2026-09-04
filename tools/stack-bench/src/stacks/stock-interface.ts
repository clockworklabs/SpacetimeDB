// The warehouse contract names the stock tables other systems write directly.
// A direct write that cannot find them is the application not providing that
// interface, not a harness fault; the marker lets the grader grade it so.
export function stockInterfaceError(message: string, options: { cause?: unknown } = {}): Error {
  return Object.assign(new Error(message, options), { stockInterface: true });
}

// A database that reports the stock table, one of its columns, or a
// referenced row as absent or unreadable is telling us the application did
// not provide the interface, whatever the engine's wording.
const INTERFACE_ABSENT = /no such (?:table|column|field)|marked private|not found|does not have a field|does not exist|unknown (?:column|field)|undefined column|invalid column/i;

export function describesMissingStockInterface(detail: string): boolean {
  return INTERFACE_ABSENT.test(detail);
}
