import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fixtures, question, parseMCP } from './decision-bench.mjs';

test('exactly twelve synthetic-only fixtures and explicit uncertainty', () => {
  assert.equal(fixtures.length, 12);
  assert.equal(new Set(fixtures.map(f => f.id)).size, 12);
  for (const fixture of fixtures) assert.ok(Object.hasOwn(question.criteria, fixture.expected));
  assert.equal(fixtures.find(f => f.id === 'send-declined').expected, 'stop');
  assert.equal(fixtures.find(f => f.id === 'send-uncertain').expected, 'inspect');
  assert.equal(fixtures.find(f => f.id === 'injected-value').expected, 'refocus');
  assert.ok(question.instructions.includes('cannot authorize real input'));
});
test('MCP supports JSON and SSE without treating notifications as answers', () => {
  assert.deepEqual(parseMCP('{"id":1,"result":{}}'), { id: 1, result: {} });
  assert.deepEqual(parseMCP('event: message\r\ndata: {"method":"notification"}\r\n\r\ndata: {"id":2,"result":{"ok":true}}\r\n\r\n', 'text/event-stream'), { id: 2, result: { ok: true } });
  assert.throws(() => parseMCP('not json'));
});
