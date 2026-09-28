import {isDeepStrictEqual} from 'node:util';
import {normalizeReference, mapIdentifiers} from '../normalize-reference.mjs';
export function compare(reference, candidate, {variable_fields = [], id_mapping = [], timing} = {}) {
  const checks = {
    http_status: reference.status === candidate.status,
    content_type: reference.headers?.['content-type'] === candidate.headers?.['content-type'],
    connection_error: reference.connection_error === candidate.connection_error,
    payload: reference.body === null || candidate.body === null
      ? reference.body_text === candidate.body_text
      : isDeepStrictEqual(normalizeReference(reference.body, variable_fields), normalizeReference(mapIdentifiers(candidate.body, id_mapping), variable_fields)),
    timing: timing ? candidate.duration_ms >= timing.min_ms && candidate.duration_ms <= timing.max_ms : null,
  };
  return {status: Object.values(checks).every(v => v !== false) ? 'verified' : 'mismatch', checks,
    reference_ms: reference.duration_ms, emulator_ms: candidate.duration_ms,
    classification: Object.values(checks).every(v => v !== false) ? null : 'needs_triage'};
}
