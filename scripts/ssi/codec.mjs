// SSI ECR JSON 1.4.6 §§1.2–1.3. Document revision != wire version.
export const prefix = Buffer.from([0x02, 0x66, 0x01]);
export const lrc = bytes => bytes.reduce((sum, byte) => sum ^ byte, 0);
export function encode(value) {
  const data = Buffer.from(JSON.stringify(value), 'utf8');
  if (data.length > 65535) throw new RangeError('SSI DATA exceeds uint16 length');
  const frame = Buffer.alloc(data.length + 6);
  prefix.copy(frame); frame.writeUInt16BE(data.length, 3);
  data.copy(frame, 5); frame[frame.length - 1] = lrc(data);
  return frame;
}

// Framing failures deliberately close the connection: searching for STX inside
// corrupt DATA could turn payload bytes into an unintended payment command.
export class Decoder {
  pending = Buffer.alloc(0);
  push(chunk) {
    this.pending = Buffer.concat([this.pending, chunk]);
    const frames = [];
    while (this.pending.length >= 5) {
      if (!this.pending.subarray(0, 3).equals(prefix)) {
        frames.push({error: 'E01', hex: this.pending.toString('hex'), fatal: true});
        this.pending = Buffer.alloc(0); break;
      }
      const length = this.pending.readUInt16BE(3) + 6;
      if (this.pending.length < length) break;
      const raw = this.pending.subarray(0, length);
      this.pending = this.pending.subarray(length);
      const data = raw.subarray(5, -1), frame = {hex: raw.toString('hex')};
      if (lrc(data) !== raw.at(-1)) frame.error = 'E02';
      else {
        try { frame.json = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(data)); }
        catch { frame.error = 'E03'; }
      }
      frames.push(frame);
    }
    return frames;
  }
}
