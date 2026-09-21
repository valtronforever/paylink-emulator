// Only explicitly named nondeterministic identifiers may be normalized.
// Preserve absence, nullability and scalar types; never mask an arbitrary object.
const allowed = new Set(['id', 'result.rrn', 'result.auth_code', 'result.receipt_no', 'result.invoice_num']);
export function normalizeReference(value, paths) {
  const copy = structuredClone(value);
  for (const path of paths) {
    if (!allowed.has(path)) throw new Error(`Not an allowed variable field: ${path}`);
    const keys = path.split('.');
    let parent = copy;
    for (const key of keys.slice(0, -1)) parent = parent?.[key];
    const key = keys.at(-1);
    if (!parent || !Object.hasOwn(parent, key) || parent[key] === null) continue;
    const kind = typeof parent[key];
    if (!['string', 'number'].includes(kind)) throw new Error(`Expected scalar identifier: ${path}`);
    parent[key] = `<variable:${kind}>`;
  }
  return copy;
}
