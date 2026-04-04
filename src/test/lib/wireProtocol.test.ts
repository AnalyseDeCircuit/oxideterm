import { describe, expect, it } from 'vitest';
import {
  MSG_TYPE_DATA,
  MSG_TYPE_RESIZE,
  MSG_TYPE_HEARTBEAT,
  MSG_TYPE_ERROR,
  HEADER_SIZE,
  encodeHeartbeatFrame,
  encodeDataFrame,
  encodeResizeFrame,
} from '@/lib/wireProtocol';

describe('constants', () => {
  it('has correct message type values', () => {
    expect(MSG_TYPE_DATA).toBe(0x00);
    expect(MSG_TYPE_RESIZE).toBe(0x01);
    expect(MSG_TYPE_HEARTBEAT).toBe(0x02);
    expect(MSG_TYPE_ERROR).toBe(0x03);
  });

  it('has correct header size', () => {
    expect(HEADER_SIZE).toBe(5);
  });
});

describe('encodeHeartbeatFrame', () => {
  it('encodes correct frame', () => {
    const frame = encodeHeartbeatFrame(42);
    expect(frame.length).toBe(HEADER_SIZE + 4); // 9 bytes

    const view = new DataView(frame.buffer);
    // Type byte
    expect(view.getUint8(0)).toBe(MSG_TYPE_HEARTBEAT);
    // Length (big-endian)
    expect(view.getUint32(1, false)).toBe(4);
    // Sequence number (big-endian)
    expect(view.getUint32(5, false)).toBe(42);
  });

  it('encodes zero sequence', () => {
    const frame = encodeHeartbeatFrame(0);
    const view = new DataView(frame.buffer);
    expect(view.getUint32(5, false)).toBe(0);
  });

  it('encodes max uint32 sequence', () => {
    const frame = encodeHeartbeatFrame(0xFFFFFFFF);
    const view = new DataView(frame.buffer);
    expect(view.getUint32(5, false)).toBe(0xFFFFFFFF);
  });
});

describe('encodeDataFrame', () => {
  it('encodes payload correctly', () => {
    const payload = new Uint8Array([0x48, 0x65, 0x6c, 0x6c, 0x6f]); // "Hello"
    const frame = encodeDataFrame(payload);
    expect(frame.length).toBe(HEADER_SIZE + 5);

    const view = new DataView(frame.buffer);
    // Type byte
    expect(view.getUint8(0)).toBe(MSG_TYPE_DATA);
    // Length (big-endian)
    expect(view.getUint32(1, false)).toBe(5);
    // Payload
    expect(frame.slice(HEADER_SIZE)).toEqual(payload);
  });

  it('encodes empty payload', () => {
    const payload = new Uint8Array([]);
    const frame = encodeDataFrame(payload);
    expect(frame.length).toBe(HEADER_SIZE);

    const view = new DataView(frame.buffer);
    expect(view.getUint8(0)).toBe(MSG_TYPE_DATA);
    expect(view.getUint32(1, false)).toBe(0);
  });

  it('encodes large payload', () => {
    const payload = new Uint8Array(10000).fill(0xAB);
    const frame = encodeDataFrame(payload);
    expect(frame.length).toBe(HEADER_SIZE + 10000);

    const view = new DataView(frame.buffer);
    expect(view.getUint32(1, false)).toBe(10000);
    expect(frame[HEADER_SIZE]).toBe(0xAB);
    expect(frame[frame.length - 1]).toBe(0xAB);
  });
});

describe('encodeResizeFrame', () => {
  it('encodes cols and rows', () => {
    const frame = encodeResizeFrame(80, 24);
    expect(frame.length).toBe(HEADER_SIZE + 4); // 9 bytes

    const view = new DataView(frame.buffer);
    // Type byte
    expect(view.getUint8(0)).toBe(MSG_TYPE_RESIZE);
    // Length (big-endian)
    expect(view.getUint32(1, false)).toBe(4);
    // Cols and rows (big-endian uint16)
    expect(view.getUint16(5, false)).toBe(80);
    expect(view.getUint16(7, false)).toBe(24);
  });

  it('encodes zero dimensions', () => {
    const frame = encodeResizeFrame(0, 0);
    const view = new DataView(frame.buffer);
    expect(view.getUint16(5, false)).toBe(0);
    expect(view.getUint16(7, false)).toBe(0);
  });

  it('encodes max uint16 dimensions', () => {
    const frame = encodeResizeFrame(65535, 65535);
    const view = new DataView(frame.buffer);
    expect(view.getUint16(5, false)).toBe(65535);
    expect(view.getUint16(7, false)).toBe(65535);
  });

  it('uses big-endian byte order', () => {
    const frame = encodeResizeFrame(0x0102, 0x0304);
    // Cols bytes at offset 5-6: 0x01, 0x02
    expect(frame[5]).toBe(0x01);
    expect(frame[6]).toBe(0x02);
    // Rows bytes at offset 7-8: 0x03, 0x04
    expect(frame[7]).toBe(0x03);
    expect(frame[8]).toBe(0x04);
  });
});
