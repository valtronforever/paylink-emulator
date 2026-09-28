import {errors} from './model.mjs';
export const spec = {
  url: 'https://www.ssi.com.ua/_files/ugd/c8ff39_c33c4ea4837842229ecb6331cb310d28.pdf',
  revision: '1.4.6', date: '2026-09-12', wire_version: 1,
  sha256: '8e4b459ade28f4ee301de7751659d4bf365f9c0f2ac3508a6745a597158e1a65',
  ambiguities: [
    '§4.3.5 says params is absent except GetLastResult, but individual method examples require params.',
    '§4.3.2 lists APPROVED_ONLINE; §5.5.1 example uses APPROVED-ONLINE.',
    '§4.3.2 describes 2/3-character responseCode; §5.5.1 example uses 0000.',
    'Request examples contain punctuation errors; codec tests use valid JSON and the §6.1 XOR algorithm.',
  ],
};
const requestErrors = new Set([0,1,2,3,4,5,6,7,8,9,18,19]);
const states = ['idle','busy','awaiting_card','awaiting_verification','bank_communication','printing','settlement_required','remove_card','awaiting_second_step'];
const financial = 'FAILED TRY_AGAIN TRY_AGAIN_PHONE CARD_ERROR MESSAGE_ERROR NO_CONNECTION CONNECTION_ERROR INTERNAL_ERROR EMV_ERROR CONNECTION_TIMEOUT INSUFFICIENT_BALANCE CARD_BLOCKED CANCELLED UNSUPPORTED_CARD DECLINED_ONLINE APPROVED_ONLINE DECLINED_OFFLINE CARD_DECLINED_ONLINE APPROVED_OFFLINE KEY_ERROR BATCH_UPLOAD_NEEDED SIGNATURE_VERIFICATION_ERROR COMPLETED APPROVED IN_PROGRESS FORCED_DECLINED_OFFLINE CANCELLED_BEFORE_START PIN_ERROR BATCH_CLOSE_NEEDED OK PRINTER_ERROR'.split(' ');
const base = {status: 'not_run', raw_ssi: null, paylink_reaction: null, emulator_case: null, fixture: null,
  reason: 'No reviewed paired reference fixture yet; injection is not proof of physical reachability.'};
export const inventory = {
  spec, scope: 'All errorCode and status values, and all transactionResult examples in this document. Bank responseCode is not exhaustively enumerated by SSI.',
  errors: errors.map((description, n) => ({...base, code: `E${String(n).padStart(2, '0')}`, description,
    section: '4.3.3', pages: [29,30], category: requestErrors.has(n) ? 'request' : 'operation',
    method: requestErrors.has(n) ? 'Purchase' : 'GetLastResult', phase: requestErrors.has(n) ? 'acceptance' : 'completed',
    injection: requestErrors.has(n) ? 'request_errors.Purchase' : 'error_code',
  })),
  states: states.map((description, n) => ({...base, code: `S0${n}`, description, section: '4.3.4', pages: [31],
    method: 'GetStatus', injection: n ? `phases[].status=S0${n}` : 'complete operation'})),
  financial_results: financial.map(code => ({...base, code, section: '4.3.2', pages: [26,27], method: 'GetLastResult', injection: 'transaction_result',
    reason: 'SSI lists result examples, not a normative mapping to errorCode/responseCode; combinations require explicit scenario evidence.'})),
  transport: ['silence','close','bad_lrc','bad_length','bad_json','fragmented_frames','coalesced_frames','unknown_status'].map(code => ({...base, code, category: 'transport_or_invalid_message'})),
  local_paylink_errors: [],
};
