class Writer {
    bytes = [];
    u8(value) {
        this.bytes.push(value & 255);
    }
    u16(value) {
        this.u8(value >> 8);
        this.u8(value);
    }
    u32(value) {
        this.u8(value >> 24);
        this.u8(value >> 16);
        this.u8(value >> 8);
        this.u8(value);
    }
    raw(value) {
        this.bytes.push(...value);
    }
    string(value) {
        const data = new TextEncoder().encode(value);
        if (data.length < 32)
            this.u8(0xa0 | data.length);
        else if (data.length < 256) {
            this.u8(0xd9);
            this.u8(data.length);
        }
        else {
            this.u8(0xda);
            this.u16(data.length);
        }
        this.raw(data);
    }
    binary(value) {
        if (value.length < 256) {
            this.u8(0xc4);
            this.u8(value.length);
        }
        else {
            this.u8(0xc5);
            this.u16(value.length);
        }
        this.raw(value);
    }
    uint(value) {
        if (value < 128)
            this.u8(value);
        else if (value < 256) {
            this.u8(0xcc);
            this.u8(value);
        }
        else if (value < 65536) {
            this.u8(0xcd);
            this.u16(value);
        }
        else {
            this.u8(0xce);
            this.u32(value);
        }
    }
    oscString(value) {
        const data = new TextEncoder().encode(value);
        this.raw(data);
        this.u8(0);
        while (this.bytes.length % 4)
            this.u8(0);
    }
    finish() {
        return Uint8Array.from(this.bytes);
    }
}
export function encodeOscPacket(message) {
    const writer = new Writer();
    writer.oscString(message.address);
    writer.oscString(`,${message.args.map(tag).join('')}`);
    for (const arg of message.args) {
        const offset = writer.bytes.length;
        if (arg.type === 'int') {
            writer.u32(arg.value);
        }
        else if (arg.type === 'int64') {
            const data = new ArrayBuffer(8);
            new DataView(data).setBigInt64(0, BigInt(arg.value), false);
            writer.raw(new Uint8Array(data));
        }
        else if (arg.type === 'float') {
            const data = new ArrayBuffer(4);
            new DataView(data).setFloat32(0, arg.value, false);
            writer.raw(new Uint8Array(data));
        }
        else if (arg.type === 'str')
            writer.oscString(arg.value);
        if (writer.bytes.length < offset)
            throw new Error('OSC encoding failed');
    }
    return writer.finish();
}
function tag(arg) {
    if (arg.type === 'int')
        return 'i';
    if (arg.type === 'int64')
        return 'h';
    if (arg.type === 'float')
        return 'f';
    if (arg.type === 'str')
        return 's';
    return arg.value ? 'T' : 'F';
}
export function encodeKepStreamFrame(payload, route) {
    const writer = new Writer();
    writer.u8(0x83);
    writer.string('t');
    writer.string('osc');
    writer.string('p');
    writer.binary(payload);
    writer.string('r');
    writer.string(route);
    const envelope = writer.finish();
    const framed = new Uint8Array(envelope.length + 4);
    new DataView(framed.buffer).setUint32(0, envelope.length, false);
    framed.set(envelope, 4);
    return framed;
}
class Reader {
    bytes;
    offset = 0;
    view;
    constructor(bytes) {
        this.bytes = bytes;
        this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    }
    byte() {
        if (this.offset >= this.bytes.length)
            throw new Error('incomplete KEP envelope');
        return this.bytes[this.offset++];
    }
    take(length) {
        const start = this.offset;
        this.offset += length;
        if (this.offset > this.bytes.length)
            throw new Error('incomplete KEP value');
        return this.bytes.slice(start, this.offset);
    }
    u16() {
        const value = this.view.getUint16(this.offset, false);
        this.offset += 2;
        return value;
    }
    u32() {
        const value = this.view.getUint32(this.offset, false);
        this.offset += 4;
        return value;
    }
    mapSize() {
        const marker = this.byte();
        if ((marker & 0xf0) === 0x80)
            return marker & 15;
        if (marker === 0xde)
            return this.u16();
        throw new Error('unsupported KEP map');
    }
    string() {
        const marker = this.byte();
        const length = (marker & 0xe0) === 0xa0
            ? marker & 31
            : marker === 0xd9
                ? this.byte()
                : marker === 0xda
                    ? this.u16()
                    : marker === 0xdb
                        ? this.u32()
                        : -1;
        if (length < 0)
            throw new Error('unsupported KEP string');
        return new TextDecoder().decode(this.take(length));
    }
    binary() {
        const marker = this.byte();
        const length = marker === 0xc4 ? this.byte() : marker === 0xc5 ? this.u16() : marker === 0xc6 ? this.u32() : -1;
        if (length < 0)
            throw new Error('unsupported KEP binary');
        return this.take(length);
    }
    skip() {
        const marker = this.bytes[this.offset];
        if ((marker & 0xe0) === 0xa0) {
            this.offset += 1 + (marker & 31);
            return;
        }
        if (marker <= 0x7f) {
            this.offset++;
            return;
        }
        if (marker === 0xcc) {
            this.offset += 2;
            return;
        }
        if (marker === 0xcd) {
            this.offset += 3;
            return;
        }
        if (marker === 0xce) {
            this.offset += 5;
            return;
        }
        if (marker === 0xc4) {
            this.offset += 2 + this.bytes[this.offset + 1];
            return;
        }
        throw new Error('unsupported KEP value');
    }
}
export function decodeKepJsonFrames(bytes) {
    const events = [];
    let offset = 0;
    while (offset < bytes.length) {
        if (bytes.length - offset < 4)
            throw new Error('incomplete KEP frame');
        const length = new DataView(bytes.buffer, bytes.byteOffset + offset, 4).getUint32(0, false);
        offset += 4;
        const reader = new Reader(bytes.slice(offset, offset + length));
        offset += length;
        let type = '';
        let payload = null;
        for (let index = 0, size = reader.mapSize(); index < size; index++) {
            const key = reader.string();
            if (key === 't')
                type = reader.string();
            else if (key === 'p')
                payload = reader.binary();
            else
                reader.skip();
        }
        if (type !== 'json' || !payload)
            throw new Error(`Unsupported KEP response payload: ${type || 'missing'}`);
        events.push(new TextDecoder().decode(payload));
    }
    return events;
}
