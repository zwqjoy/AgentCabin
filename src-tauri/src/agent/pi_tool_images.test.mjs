import assert from 'node:assert/strict';
import { deflateSync } from 'node:zlib';
import { test } from 'node:test';
import { sanitizeToolImages } from './pi_expert_extension.mjs';

function crc(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit++) value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
  }
  return (value ^ 0xffffffff) >>> 0;
}
function chunk(type, data, corrupt = false) {
  const body = Buffer.concat([Buffer.from(type), data]);
  const size = Buffer.alloc(4); size.writeUInt32BE(data.length);
  const sum = Buffer.alloc(4); sum.writeUInt32BE(corrupt ? 0 : crc(body));
  return Buffer.concat([size, body, sum]);
}
function image(optionalCorrupt = false, pixelsCorrupt = false) {
  const header = Buffer.alloc(13); header.writeUInt32BE(1, 0); header.writeUInt32BE(1, 4); header[8] = 8; header[9] = 6;
  return { type: 'image', mimeType: 'image/png', data: Buffer.concat([
    Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header),
    chunk('iCCP', Buffer.from('fixture'), optionalCorrupt),
    chunk('IDAT', deflateSync(Buffer.from([0,255,0,0,255])), pixelsCorrupt), chunk('IEND', Buffer.alloc(0)),
  ]).toString('base64') };
}
test('valid PNG and text remain unchanged', () => {
  const parts = [image(), {type:'text', text:'echo'}];
  assert.deepEqual(sanitizeToolImages(parts), parts);
});
test('damaged optional metadata is removed without changing pixel chunks', () => {
  const original = image(true);
  const result = sanitizeToolImages([original])[0];
  assert.equal(result.type, 'image');
  assert(!Buffer.from(result.data, 'base64').includes(Buffer.from('iCCP')));
  assert.deepEqual(sanitizeToolImages([result]), [result]);
});
test('corrupt pixels and truncated PNG become actionable text', () => {
  for (const part of [image(false,true), {...image(), data:'broken'}]) {
    assert.equal(sanitizeToolImages([part])[0].type, 'text');
  }
});
test('persisted invalid image can be sanitized repeatedly without blocking next turns', () => {
  const history = [image(true), image(false,true), {type:'text',text:'remaining result'}];
  const next = sanitizeToolImages(history);
  assert.equal(next[0].type, 'image');
  assert.equal(next[1].type, 'text');
  assert.deepEqual(sanitizeToolImages(next), next);
  assert.equal(history[1].type, 'image');
});
