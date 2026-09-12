import type { Readable, Writable } from 'node:stream';
import { randomUUID } from 'node:crypto';
import type { RpcMethods } from '../generated/contracts.js';
import { contract, MAX_FRAME, MAX_PENDING, RpcError, validate } from './validation.js';

type Pending = { resolve(value: unknown): void; reject(error: Error): void; timer: NodeJS.Timeout; output: string };
type Handler = (params: unknown) => Promise<unknown>;

/** A bounded duplex peer. A pending agent.run never blocks incoming tool calls. */
export class RpcPeer<Methods extends { [K in keyof Methods]: { input: unknown; output: unknown } } = RpcMethods> {
  private buffer = Buffer.alloc(0);
  private pending = new Map<string, Pending>();
  private handlers = new Map<string, Handler>();
  private inflight = new Set<string>();
  private writing: Promise<void> = Promise.resolve();
  private queued = 0;
  private closed = false;
  onClose: (error: Error) => void = () => {};

  constructor(private readonly input: Readable, private readonly output: Writable, private readonly timeoutMs = 30_000, private readonly protocol: 'worker' | 'host' = 'worker') {
    input.on('data', (chunk: Buffer) => this.receive(Buffer.from(chunk)));
    input.on('end', () => this.close(new RpcError(-32010, 'bridge input closed')));
    input.on('error', error => this.close(error));
    output.on('error', error => this.close(error));
  }

  handle<M extends keyof Methods & string>(method: M, handler: (params: Methods[M]['input']) => Promise<Methods[M]['output']>): void {
    if (this.handlers.has(method)) throw new Error(`duplicate handler ${method}`);
    this.handlers.set(method, handler as Handler);
  }

  async call<M extends keyof Methods & string>(method: M, params: Methods[M]['input'], signal?: AbortSignal): Promise<Methods[M]['output']> {
    if (signal?.aborted) throw new RpcError(-32011, 'request cancelled');
    if (this.closed) throw new RpcError(-32010, 'bridge closed');
    if (this.pending.size >= MAX_PENDING) throw new RpcError(-32005, 'pending request limit reached');
    const [input, output] = contract(method, this.protocol); validate(input, params);
    const id = randomUUID();
    let abort: (() => void) | undefined;
    const result = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id); reject(new RpcError(-32012, `${method} timed out; effects require lookup before retry`));
      }, this.timeoutMs);
      this.pending.set(id, { resolve, reject, timer, output });
      abort = () => {
        clearTimeout(timer); this.pending.delete(id); reject(new RpcError(-32011, 'request cancelled; effect outcome may be unknown'));
      };
      signal?.addEventListener('abort', abort, { once: true });
      void this.send({ jsonrpc: '2.0', id, method, params }).catch(error => { clearTimeout(timer); this.pending.delete(id); reject(error); });
    });
    try { return await result as Methods[M]['output']; }
    finally { if (abort) signal?.removeEventListener('abort', abort); }
  }

  close(error: Error = new RpcError(-32010, 'bridge closed')): void {
    if (this.closed) return;
    this.closed = true; this.buffer = Buffer.alloc(0);
    for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(error); }
    this.pending.clear(); this.onClose(error);
  }

  private receive(chunk: Buffer): void {
    if (this.closed) return;
    // Split before accumulation so one huge unterminated line never allocates
    // an unbounded frame. Multiple complete lines in a read remain valid.
    while (chunk.length) {
      const newline = chunk.indexOf(10);
      const length = newline < 0 ? chunk.length : newline;
      if (this.buffer.length + length > MAX_FRAME) { this.close(new RpcError(-32600, 'frame too large')); return; }
      this.buffer = Buffer.concat([this.buffer, chunk.subarray(0, length)]);
      if (newline < 0) break;
      const frame = this.buffer; this.buffer = Buffer.alloc(0);
      chunk = chunk.subarray(newline + 1);
      let value: unknown;
      try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(frame)); }
      catch { void this.send({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'invalid UTF-8 JSON frame' } }).catch(error => this.close(error)); continue; }
      void this.dispatch(value).catch(error => this.close(error));
    }
  }

  private async dispatch(value: unknown): Promise<void> {
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new RpcError(-32600, 'invalid envelope');
    const msg = value as Record<string, unknown>;
    if (msg.jsonrpc !== '2.0' || typeof msg.id !== 'string') throw new RpcError(-32600, 'envelope requires JSON-RPC 2.0 and a string id');
    if (typeof msg.method === 'string') {
      if (Object.keys(msg).some(k => !['jsonrpc','id','method','params'].includes(k))) throw new RpcError(-32600, 'unknown request envelope field');
      if (this.inflight.has(msg.id) || this.inflight.size >= MAX_PENDING) throw new RpcError(-32005, 'duplicate or excessive active request IDs');
      this.inflight.add(msg.id);
      try {
        const [input, output] = contract(msg.method, this.protocol); validate(input, msg.params);
        const handler = this.handlers.get(msg.method);
        if (!handler) throw new RpcError(-32601, 'method not available on this peer');
        const result = await handler(msg.params); validate(output, result);
        await this.send({ jsonrpc: '2.0', id: msg.id, result });
      } catch (error) {
        await this.send({ jsonrpc: '2.0', id: msg.id, error: { code: error instanceof RpcError ? error.code : -32603, message: error instanceof Error ? error.message : 'request failed' } });
      } finally { this.inflight.delete(msg.id); }
    } else {
      if (Object.keys(msg).some(k => !['jsonrpc','id','result','error'].includes(k)) || ('result' in msg) === ('error' in msg)) throw new RpcError(-32600, 'invalid response envelope');
      const pending = this.pending.get(msg.id);
      if (!pending) return; // Late response after a local timeout/cancellation.
      this.pending.delete(msg.id); clearTimeout(pending.timer);
      if ('error' in msg) {
        const error = msg.error as Record<string, unknown> | null;
        pending.reject(new RpcError(typeof error?.code === 'number' ? error.code : -32603, typeof error?.message === 'string' ? error.message : 'malformed RPC error'));
      } else {
        try { validate(pending.output, msg.result); pending.resolve(msg.result); }
        catch (error) { pending.reject(error as Error); }
      }
    }
  }

  private async send(value: unknown): Promise<void> {
    if (this.closed) throw new RpcError(-32010, 'bridge closed');
    const frame = JSON.stringify(value) + '\n';
    if (Buffer.byteLength(frame) > MAX_FRAME || this.queued >= MAX_PENDING) throw new RpcError(-32005, 'outbound bridge capacity exceeded');
    this.queued++;
    const write = this.writing.then(() => new Promise<void>((resolve, reject) => {
      this.output.write(frame, error => error ? reject(error) : resolve());
    }));
    this.writing = write.catch(() => {});
    try { await write; } finally { this.queued--; }
  }
}
