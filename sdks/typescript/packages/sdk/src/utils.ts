export function toPascalCase(s: string): string {
  const str = s.replace(/([-_][a-z])/gi, $1 => {
    return $1.toUpperCase().replace('-', '').replace('_', '');
  });

  return str.charAt(0).toUpperCase() + str.slice(1);
}

export function deepEqual(obj1: any, obj2: any): boolean {
  // If both are strictly equal (covers primitives and reference equality), return true
  if (obj1 === obj2) return true;

  // If either is a primitive type or one is null, return false since we already checked for strict equality
  if (
    typeof obj1 !== 'object' ||
    obj1 === null ||
    typeof obj2 !== 'object' ||
    obj2 === null
  ) {
    return false;
  }

  // Get keys of both objects
  const keys1 = Object.keys(obj1);
  const keys2 = Object.keys(obj2);

  // If number of keys is different, return false
  if (keys1.length !== keys2.length) return false;

  // Check all keys and compare values recursively
  for (let key of keys1) {
    if (!keys2.includes(key) || !deepEqual(obj1[key], obj2[key])) {
      return false;
    }
  }

  return true;
}
