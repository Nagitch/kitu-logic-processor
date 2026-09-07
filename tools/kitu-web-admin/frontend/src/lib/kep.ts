import type { ClientOscMessage, JsonOscArg } from "./types";

export type KepEnvelopeInput = {
  payloadType: string;
  route?: string;
  correlationId?: number;
  flags?: number;
  payload: Uint8Array;
};

export type KepEnvelope = Required<
  Pick<KepEnvelopeInput, "payloadType" | "payload">
> &
  Partial<Pick<KepEnvelopeInput, "route" | "correlationId" | "flags">>;

export function encodeKepEnvelope(envelope: KepEnvelopeInput): Uint8Array {
  const fields: Array<[string, string | number | Uint8Array]> = [
    ["t", envelope.payloadType],
    ["p", envelope.payload],
  ];

  if (envelope.route !== undefined) fields.push(["r", envelope.route]);
  if (envelope.correlationId !== undefined) {
    fields.push(["i", envelope.correlationId]);
  }
  if (envelope.flags !== undefined) fields.push(["f", envelope.flags]);

  const writer = new ByteWriter();
  writer.writeMapHeader(fields.length);
  for (const [key, value] of fields) {
    writer.writeString(key);
    if (typeof value === "string") {
      writer.writeString(value);
    } else if (typeof value === "number") {
      writer.writeUint(value);
    } else {
      writer.writeBinary(value);
    }
  }
  return writer.finish();
}

export function encodeKepStreamFrame(envelope: KepEnvelopeInput): Uint8Array {
  const envelopeBytes = encodeKepEnvelope(envelope);
  const frame = new Uint8Array(4 + envelopeBytes.length);
  new DataView(frame.buffer, frame.byteOffset, frame.byteLength).setUint32(
    0,
    envelopeBytes.length,
    false,
  );
  frame.set(envelopeBytes, 4);
  return frame;
}

export function decodeKepEnvelope(bytes: Uint8Array): KepEnvelope {
  const reader = new ByteReader(bytes);
  const size = reader.readMapHeader();
  const envelope: Partial<KepEnvelope> = {};

  for (let index = 0; index < size; index += 1) {
    const key = reader.readString();
    switch (key) {
      case "t":
        envelope.payloadType = reader.readString();
        break;
      case "r":
        envelope.route = reader.readString();
        break;
      case "i":
        envelope.correlationId = reader.readUint();
        break;
      case "f":
        envelope.flags = reader.readUint();
        break;
      case "p":
        envelope.payload = reader.readBinary();
        break;
      default:
        reader.skipValue();
        break;
    }
  }

  if (!envelope.payloadType) throw new Error("KEP envelope is missing t");
  if (!envelope.payload) throw new Error("KEP envelope is missing p");
  return envelope as KepEnvelope;
}

export function decodeKepStreamFrames(bytes: Uint8Array): KepEnvelope[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const envelopes: KepEnvelope[] = [];
  let offset = 0;

  while (offset < bytes.length) {
    if (bytes.length - offset < 4) {
      throw new Error("incomplete KEP stream frame length");
    }

    const length = view.getUint32(offset, false);
    offset += 4;
    if (bytes.length - offset < length) {
      throw new Error(
        `incomplete KEP stream frame payload: expected ${length} bytes, got ${
          bytes.length - offset
        } bytes`,
      );
    }

    envelopes.push(decodeKepEnvelope(bytes.slice(offset, offset + length)));
    offset += length;
  }

  return envelopes;
}

export function encodeOscPacket(message: ClientOscMessage): Uint8Array {
  const writer = new ByteWriter();
  writer.writeOscString(message.address);

  const typeTags = [","];
  for (const arg of message.args) {
    typeTags.push(oscTypeTag(arg));
  }
  writer.writeOscString(typeTags.join(""));

  for (const arg of message.args) {
    switch (arg.type) {
      case "int":
        writer.writeInt32(arg.value);
        break;
      case "int64":
        writer.writeInt64(arg.value);
        break;
      case "float":
        writer.writeFloat32(arg.value);
        break;
      case "str":
        writer.writeOscString(arg.value);
        break;
      case "bool":
        break;
    }
  }

  return writer.finish();
}

function oscTypeTag(arg: JsonOscArg) {
  switch (arg.type) {
    case "int":
      return "i";
    case "int64":
      return "h";
    case "float":
      return "f";
    case "str":
      return "s";
    case "bool":
      return arg.value ? "T" : "F";
  }
}

class ByteReader {
  #offset = 0;
  #textDecoder = new TextDecoder();
  #view: DataView;

  constructor(private readonly bytes: Uint8Array) {
    this.#view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  readMapHeader() {
    const marker = this.readByte();
    if ((marker & 0xf0) === 0x80) return marker & 0x0f;
    if (marker === 0xde) return this.readUint16();
    throw new Error(`unsupported KEP map marker: 0x${marker.toString(16)}`);
  }

  readString() {
    const marker = this.readByte();
    let length: number;
    if ((marker & 0xe0) === 0xa0) {
      length = marker & 0x1f;
    } else if (marker === 0xd9) {
      length = this.readByte();
    } else if (marker === 0xda) {
      length = this.readUint16();
    } else if (marker === 0xdb) {
      length = this.readUint32();
    } else {
      throw new Error(
        `unsupported KEP string marker: 0x${marker.toString(16)}`,
      );
    }

    return this.#textDecoder.decode(this.readBytes(length));
  }

  readUint() {
    const marker = this.readByte();
    if (marker <= 0x7f) return marker;
    if (marker === 0xcc) return this.readByte();
    if (marker === 0xcd) return this.readUint16();
    if (marker === 0xce) return this.readUint32();
    if (marker === 0xcf) {
      const value = this.#view.getBigUint64(this.take(8), false);
      if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
        throw new Error("KEP uint exceeds JavaScript safe integer range");
      }
      return Number(value);
    }
    throw new Error(`unsupported KEP uint marker: 0x${marker.toString(16)}`);
  }

  readBinary() {
    const marker = this.readByte();
    let length: number;
    if (marker === 0xc4) {
      length = this.readByte();
    } else if (marker === 0xc5) {
      length = this.readUint16();
    } else if (marker === 0xc6) {
      length = this.readUint32();
    } else {
      throw new Error(
        `unsupported KEP binary marker: 0x${marker.toString(16)}`,
      );
    }
    return this.readBytes(length);
  }

  skipValue() {
    const marker = this.peekByte();
    if ((marker & 0xe0) === 0xa0) {
      this.readString();
    } else if ((marker & 0xf0) === 0x80) {
      const size = this.readMapHeader();
      for (let index = 0; index < size; index += 1) {
        this.skipValue();
        this.skipValue();
      }
    } else if (marker <= 0x7f || [0xcc, 0xcd, 0xce, 0xcf].includes(marker)) {
      this.readUint();
    } else if ([0xd9, 0xda, 0xdb].includes(marker)) {
      this.readString();
    } else if ([0xc4, 0xc5, 0xc6].includes(marker)) {
      this.readBinary();
    } else if (marker === 0xc0) {
      this.readByte();
    } else {
      throw new Error(`unsupported KEP field marker: 0x${marker.toString(16)}`);
    }
  }

  readByte() {
    return this.bytes[this.take(1)];
  }

  peekByte() {
    if (this.#offset >= this.bytes.length) {
      throw new Error("unexpected end of KEP bytes");
    }
    return this.bytes[this.#offset];
  }

  readUint16() {
    return this.#view.getUint16(this.take(2), false);
  }

  readUint32() {
    return this.#view.getUint32(this.take(4), false);
  }

  readBytes(length: number) {
    const offset = this.take(length);
    return this.bytes.slice(offset, offset + length);
  }

  take(length: number) {
    if (this.#offset + length > this.bytes.length) {
      throw new Error("unexpected end of KEP bytes");
    }
    const offset = this.#offset;
    this.#offset += length;
    return offset;
  }
}

class ByteWriter {
  #bytes: number[] = [];
  #textEncoder = new TextEncoder();
  #scratch = new ArrayBuffer(8);
  #view = new DataView(this.#scratch);

  writeMapHeader(size: number) {
    if (size <= 15) {
      this.#bytes.push(0x80 | size);
      return;
    }
    this.#bytes.push(0xde, (size >> 8) & 0xff, size & 0xff);
  }

  writeString(value: string) {
    const encoded = this.#textEncoder.encode(value);
    if (encoded.length <= 31) {
      this.#bytes.push(0xa0 | encoded.length);
    } else if (encoded.length <= 0xff) {
      this.#bytes.push(0xd9, encoded.length);
    } else if (encoded.length <= 0xffff) {
      this.#bytes.push(
        0xda,
        (encoded.length >> 8) & 0xff,
        encoded.length & 0xff,
      );
    } else {
      throw new Error("KEP string is too large");
    }
    this.writeBytes(encoded);
  }

  writeUint(value: number) {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new Error("KEP integer must be a non-negative safe integer");
    }
    if (value <= 0x7f) {
      this.#bytes.push(value);
    } else if (value <= 0xff) {
      this.#bytes.push(0xcc, value);
    } else if (value <= 0xffff) {
      this.#bytes.push(0xcd, (value >> 8) & 0xff, value & 0xff);
    } else if (value <= 0xffffffff) {
      this.#bytes.push(
        0xce,
        (value >>> 24) & 0xff,
        (value >>> 16) & 0xff,
        (value >>> 8) & 0xff,
        value & 0xff,
      );
    } else {
      const high = Math.floor(value / 0x100000000);
      const low = value >>> 0;
      this.#bytes.push(
        0xcf,
        (high >>> 24) & 0xff,
        (high >>> 16) & 0xff,
        (high >>> 8) & 0xff,
        high & 0xff,
        (low >>> 24) & 0xff,
        (low >>> 16) & 0xff,
        (low >>> 8) & 0xff,
        low & 0xff,
      );
    }
  }

  writeBinary(value: Uint8Array) {
    if (value.length <= 0xff) {
      this.#bytes.push(0xc4, value.length);
    } else if (value.length <= 0xffff) {
      this.#bytes.push(0xc5, (value.length >> 8) & 0xff, value.length & 0xff);
    } else {
      this.#bytes.push(
        0xc6,
        (value.length >>> 24) & 0xff,
        (value.length >>> 16) & 0xff,
        (value.length >>> 8) & 0xff,
        value.length & 0xff,
      );
    }
    this.writeBytes(value);
  }

  writeOscString(value: string) {
    this.writeBytes(this.#textEncoder.encode(value));
    this.#bytes.push(0);
    while (this.#bytes.length % 4 !== 0) {
      this.#bytes.push(0);
    }
  }

  writeInt32(value: number) {
    this.#view.setInt32(0, value, false);
    this.writeBytes(new Uint8Array(this.#scratch, 0, 4));
  }

  writeInt64(value: number) {
    if (!Number.isSafeInteger(value)) {
      throw new Error("OSC int64 must be a safe integer");
    }
    this.#view.setBigInt64(0, BigInt(value), false);
    this.writeBytes(new Uint8Array(this.#scratch, 0, 8));
  }

  writeFloat32(value: number) {
    this.#view.setFloat32(0, value, false);
    this.writeBytes(new Uint8Array(this.#scratch, 0, 4));
  }

  writeBytes(bytes: Uint8Array) {
    for (const byte of bytes) {
      this.#bytes.push(byte);
    }
  }

  finish() {
    return new Uint8Array(this.#bytes);
  }
}
