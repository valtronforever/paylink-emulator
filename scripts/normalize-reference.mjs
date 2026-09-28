// Only explicitly named nondeterministic identifiers may be normalized.
// Preserve absence, nullability and scalar types; never mask an arbitrary object.
const allowed = new Set(['id', 'result.rrn', 'result.auth_code', 'result.receipt_no', 'result.invoice_num', 'result.date_time', 'result.additional_properties.transactionUid']);
export function normalizeReference(value, paths) {
  if (value?.result?.additional_properties?.transactionUid !== undefined && value.result.additional_properties.transactionUid !== value.id) {
    throw new Error('Broken operation ID / SSI transactionUid correlation');
  }
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

export function mapIdentifiers(value, mappings = []) {
  const copy = structuredClone(value);
  const fields = new Set(['result.terminal', 'result.terminal_id', 'result.merchant_id']);
  for (const mapping of mappings) {
    if (!Array.isArray(mapping.paths) || typeof mapping.reference !== 'string' || typeof mapping.emulator !== 'string') throw Error('Invalid ID mapping');
    for (const path of mapping.paths) {
      if (!fields.has(path)) throw Error(`Not an ID mapping field: ${path}`);
      const [parent, key] = path.split('.');
      if (copy?.[parent]?.[key] === mapping.emulator) copy[parent][key] = mapping.reference;
    }
  }
  return copy;
}
