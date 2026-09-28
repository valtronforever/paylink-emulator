// Independent terminal-side model; never imports the HTTP emulator's mapper.
export const errors = [
  'General error', 'Protocol version is not supported by the terminal', 'Checksum error',
  'Json message format error', 'Required fields are missing', 'Unknown method', 'Terminal Busy',
  'Merchant Id is not found', 'Interrupt prohibited', 'Format error', 'Connection error',
  'Verification error (pin, signature)', 'Transaction canceled', 'MAC Error - MAC', 'Key error',
  'Need z report', 'Card error', 'EMV error', 'Printer is out of paper or inoperational',
  'Transaction amount out of limit', 'No last transactions found', 'General error in transaction',
  'Transaction with such transactionUid is already processing',
];
export function reply(method, code = '', params) {
  return {method, error: !!code, errorCode: code,
    errorDescription: code ? errors[Number(code.slice(1))] : '',
    ...(params === undefined ? {} : {params})};
}
export const defaultScenario = {
  id: 'approved', phases: [{status: 'S02', ms: 100}, {status: 'S04', ms: 100}],
  error_code: '', response_code: '0000', transaction_result: 'APPROVED-ONLINE',
  faults: {}, request_errors: {},
};
const methods = ['Purchase', 'PingDevice', 'GetStatus', 'GetLastResult', 'Interrupt',
  'GetMerchantList', 'GetMerchantListDetailed', 'GetTerminalInfo', 'GetResultByUid'];
const faultTypes = ['normal', 'silence', 'close', 'bad_lrc', 'bad_json', 'bad_length', 'unknown_status'];
export function scenario(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error('Expected scenario object');
  const allowed = [...Object.keys(defaultScenario), 'response_delay_ms', 'fragment_bytes', 'fragment_delay_ms', 'result_fields', 'method_delays'];
  for (const key of Object.keys(value)) if (!allowed.includes(key)) throw Error(`Unknown scenario field: ${key}`);
  const s = structuredClone({...defaultScenario, ...value});
  s.method_delays ??= {};
  if (!s.method_delays || typeof s.method_delays !== 'object' || Array.isArray(s.method_delays)) throw Error('Invalid method_delays');
  for (const [method, delay] of Object.entries(s.method_delays)) {
    if (!methods.includes(method) || !Number.isInteger(delay) || delay < 0 || delay > 3600000) throw Error('Invalid method delay');
  }
  s.result_fields ??= {};
  if (!s.result_fields || typeof s.result_fields !== 'object' || Array.isArray(s.result_fields)) throw Error('Invalid result_fields');
  for (const [key, value] of Object.entries(s.result_fields)) {
    if (!['terminalId', 'pan', 'cardHolderName', 'bankName'].includes(key) || typeof value !== 'string' || value.length > 100) throw Error('Invalid result field');
  }
  if (typeof s.id !== 'string' || !/^[a-zA-Z0-9_-]{1,80}$/.test(s.id)) throw Error('Invalid scenario id');
  if (!Array.isArray(s.phases) || s.phases.length > 20) throw Error('Invalid phases');
  for (const p of s.phases) {
    if (!/^S0[1-8]$/.test(p.status) || !Number.isInteger(p.ms) || p.ms < 0 || p.ms > 3600000) throw Error('Invalid phase');
  }
  if (typeof s.error_code !== 'string' || (s.error_code && !/^E(0\d|1\d|2[0-2])$/.test(s.error_code))) throw Error('Invalid error_code');
  for (const key of ['response_code', 'transaction_result']) if (typeof s[key] !== 'string' || s[key].length > 80) throw Error(`Invalid ${key}`);
  for (const key of ['response_delay_ms', 'fragment_delay_ms', 'fragment_bytes']) {
    s[key] ??= 0;
    if (!Number.isInteger(s[key]) || s[key] < 0 || s[key] > 3600000) throw Error(`Invalid ${key}`);
  }
  for (const key of ['faults', 'request_errors']) {
    if (!s[key] || typeof s[key] !== 'object' || Array.isArray(s[key])) throw Error(`Invalid ${key}`);
    for (const [method, value] of Object.entries(s[key])) {
      if (!methods.includes(method)) throw Error(`Unsupported method: ${method}`);
      if (key === 'faults' ? !faultTypes.includes(value) : !/^E(0\d|1\d|2[0-2])$/.test(value)) throw Error(`Invalid ${key} value`);
    }
  }
  return s;
}

export class Terminal {
  constructor(emit = () => {}, clock = () => performance.now()) {
    this.emit = emit; this.clock = clock; this.generation = 0; this.reset();
  }
  reset({preserve_result = false} = {}) {
    this.generation++; this.armed = scenario(defaultScenario); this.active = null;
    if (!preserve_result) this.last = null;
    this.emit('reset', {generation: this.generation, preserve_result});
  }
  arm(value) { this.armed = scenario(value); this.emit('armed', {scenario: this.armed}); }
  tick() {
    const op = this.active;
    if (!op) return 'S00';
    let elapsed = this.clock() - op.start;
    for (const phase of op.scenario.phases) {
      if (elapsed < phase.ms) {
        if (op.status !== phase.status) {
          op.status = phase.status; this.emit('phase', {scenario_id: op.scenario.id, status: phase.status});
        }
        return phase.status;
      }
      elapsed -= phase.ms;
    }
    this.complete(op.scenario.error_code); return 'S00';
  }
  complete(code) {
    const op = this.active;
    if (!op) return;
    this.last = {params: op.result, code, scenario: op.scenario}; this.active = null;
    this.emit('completed', {scenario_id: op.scenario.id, error_code: code, result: op.result});
  }
  state() {
    const status = this.tick();
    return {generation: this.generation, status, armed: this.armed, active: this.active, last: this.last};
  }
  handle(request) {
    const status = this.tick();
    const method = typeof request?.method === 'string' ? request.method : 'Unknown';
    if (typeof request?.method !== 'string') return reply(method, 'E04');
    if (!methods.includes(method)) { this.emit('unknown_method', {request}); return reply(method, 'E05'); }
    if (request.params !== undefined && (!request.params || typeof request.params !== 'object' || Array.isArray(request.params))) return reply(method, 'E09');
    const s = this.active?.scenario ?? this.armed;
    if (s.request_errors[method]) return reply(method, s.request_errors[method]);
    switch (method) {
      case 'PingDevice': return reply(method, '', {});
      case 'GetStatus': return {...reply(method, '', {}), status};
      case 'GetMerchantList': return reply(method, '', {merchantCount: 1, merchantList: ['TEST-MERCHANT']});
      case 'GetMerchantListDetailed': return reply(method, '', {merchantCount: 1,
        merchantList: [{merchantId: 'TEST-MERCHANT', terminalId: 'TEST0001', merchantName: 'SSI simulator'}]});
      case 'GetTerminalInfo': return reply(method, '', {
        terminalModel: 'Verifone X990', terminalSerialNumber: 'SYNTHETIC001', sdkVersion: 25,
        androidVersion: '7.1.2', securityDriver: {name: 'Simulator', versionName: '1'},
        currentApp: {name: 'SSI simulator', packageName: 'local.ssi.simulator', versionName: '1'},
        serviceApps: [], thirdPartyApps: [], paymentApps: [], externalRegisterApps: [],
        rom: {name: 'Simulator', versionName: '1'},
      });
      case 'Interrupt':
        if (!['S02', 'S03', 'S08'].includes(status)) return reply(method, 'E08');
        this.active.result.responseCode = ''; this.active.result.transactionResult = '';
        this.complete('E12'); return reply(method, '', {});
      case 'GetLastResult':
        if (this.active) return reply(method, 'E06');
        return this.last ? reply(method, this.last.code, this.last.params) : reply(method, '', {});
      case 'GetResultByUid':
        if (typeof request.params?.transactionUid !== 'string') return reply(method, 'E04');
        if (this.active?.result.transactionUid === request.params.transactionUid) return reply(method, 'E22');
        return this.last?.params.transactionUid === request.params.transactionUid
          ? reply(method, this.last.code, this.last.params) : reply(method, 'E20');
      case 'Purchase': {
        if (this.active) return reply(method, 'E06');
        const p = request.params;
        if (!p || ['transAmount', 'transCurrency', 'merchantId'].some(k => p[k] === undefined)) return reply(method, 'E04');
        if (!/^\d+$/.test(p.transAmount) || typeof p.transAmount !== 'string' || p.transCurrency !== '980' ||
            typeof p.merchantId !== 'string' || (request.step !== undefined && request.step !== '1')) return reply(method, 'E09');
        if (p.merchantId !== 'TEST-MERCHANT') return reply(method, 'E07');
        if (BigInt(p.transAmount) < 1n || BigInt(p.transAmount) > 999999999n) return reply(method, 'E19');
        this.last = null;
        const stamp = new Date(), snapshot = structuredClone(this.armed);
        const result = {originalTrnName: 'Purchase', transAmount: p.transAmount,
          authCode: snapshot.error_code ? '' : '123456', date: stamp.toLocaleDateString('en-GB', {timeZone: 'UTC'}),
          time: stamp.toISOString().slice(11, 19), invoiceNum: '000001', bankName: 'SIMULATED',
          merchantId: p.merchantId, pan: '4111XXXXXXXX1111', responseCode: snapshot.response_code,
          rrn: '000000000001', transactionResult: snapshot.transaction_result, terminalId: 'TEST0001',
          posEntryMode: 'CONTACTLESS', binName: 'VISA', cardHolderName: 'TEST USER', errorDetails: '',
          ...snapshot.result_fields, ...(p.transactionUid === undefined ? {} : {transactionUid: p.transactionUid})};
        this.active = {start: this.clock(), scenario: snapshot, result};
        this.emit('accepted', {scenario_id: snapshot.id, request});
        return reply(method, '', {});
      }
    }
  }
}
