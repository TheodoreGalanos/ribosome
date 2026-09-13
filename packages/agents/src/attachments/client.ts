import { randomUUID } from 'node:crypto';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { isAbsolute } from 'node:path';
import type { Readable, Writable } from 'node:stream';
import { RpcPeer } from '../client/rpc.js';
import { validate } from '../client/validation.js';
import type { Attachment as SavedAttachment, AttachmentFeedback, AttachmentStatus, EffectSettlementRequest, HarnessEvent, HostHello, HostRpcMethods, RecordEnvelope, RepairHandoff } from '../generated/contracts.js';
import { bounded, WriteCoordinator } from './coordinator.js';

export interface HarnessEventSource {
  subscribe(emit: (event: HarnessEvent) => Promise<void>): (() => void) | Promise<() => void>;
}

export interface AttachOptions {
  id?: string;
  executionId: string;
  connector?: string;
  connectorVersion?: string;
  start?: 'now' | 'history';
  source?: HarnessEventSource;
  onFeedback(feedback: AttachmentFeedback): Promise<void | 'rejected'> | void | 'rejected';
  onError?(error: Error): void;
  steer?(message: string): Promise<void> | void;
  coordinatedWrites?: boolean;
}

export class AttachmentClient {
  readonly attachments = new Set<HarnessAttachment>();
  private child: ChildProcessWithoutNullStreams | undefined;
  private constructor(readonly peer: RpcPeer<HostRpcMethods>, readonly hello: HostHello) {}

  static async connect(input: Readable, output: Writable, timeoutMs = 1000): Promise<AttachmentClient> {
    const peer = new RpcPeer<HostRpcMethods>(input, output, timeoutMs, 'host');
    try {
      const hello = await peer.call('host.hello', { protocol: 'ribosome-host/1' });
      const client = new AttachmentClient(peer, hello);
      peer.onClose = error => { for (const attachment of client.attachments) attachment.connectionLost(error); };
      return client;
    } catch (error) { peer.close(); throw error; }
  }

  static async start(options: { executable: string; config: string; environment?: NodeJS.ProcessEnv; timeoutMs?: number; onDiagnostic?: (text: string) => void }): Promise<AttachmentClient> {
    if (!isAbsolute(options.executable) || !isAbsolute(options.config)) throw new Error('Host executable and configuration paths must be absolute');
    const child = spawn(options.executable, ['host', options.config], { env: options.environment ?? process.env, stdio: ['pipe', 'pipe', 'pipe'] });
    let startupError: Error | undefined;
    child.on('error', error => { startupError = error; child.stdout.destroy(error); });
    child.stderr.on('data', chunk => options.onDiagnostic?.(String(chunk).slice(0, 2000)));
    try {
      // A new Rust process may still be compiling its contract validators.
      const client = await AttachmentClient.connect(child.stdout, child.stdin, options.timeoutMs ?? 5000);
      client.child = child;
      return client;
    } catch (error) { child.kill(); throw startupError ?? error; }
  }

  inspectEffect(operationId: string) {
    return this.peer.call('effect.inspect', { id: operationId });
  }

  /** Owner-only decision: stop and inspect the external executor before calling. */
  settleEffect(request: EffectSettlementRequest) {
    return this.peer.call('effect.settle', request);
  }

  async attach(options: AttachOptions): Promise<HarnessAttachment> {
    const capabilities: SavedAttachment['capabilities'] = ['observe'];
    if (options.steer) capabilities.push('steer');
    if (options.coordinatedWrites) capabilities.push('coordinated_write');
    const saved = await this.peer.call('attachment.open', {
      id: options.id ?? randomUUID(), execution_id: options.executionId,
      connector: options.connector ?? 'custom', connector_version: options.connectorVersion ?? '1',
      start: options.start ?? 'now', capabilities,
    });
    const attachment = new HarnessAttachment(this, saved, options);
    this.attachments.add(attachment);
    try { await attachment.start(); return attachment; }
    catch (error) { await attachment.detach().catch(() => {}); throw error; }
  }

  async close(): Promise<void> {
    const results = await Promise.allSettled([...this.attachments].map(a => a.detach()));
    this.child?.stdin.end();
    if (this.child && this.child.exitCode === null && this.child.signalCode === null) {
      const child = this.child;
      try { await bounded(new Promise<void>(resolve => child.once('exit', () => resolve())), 5000, 'Host shutdown timed out'); }
      catch (error) { child.kill(); this.peer.close(); throw error; }
    }
    this.peer.close();
    const failed = results.find((r): r is PromiseRejectedResult => r.status === 'rejected');
    if (failed) throw failed.reason;
  }
}

export class HarnessAttachment {
  readonly id: string;
  private unsubscribe: (() => void) | undefined;
  private stopped = false;
  private detached = false;
  private timer: NodeJS.Timeout | undefined;
  private polling: Promise<void> | undefined;
  private pending = new Set<Promise<void>>();
  private cursor = 0n;
  private failure: Error | undefined;
  private handoff: RepairHandoff | undefined;

  constructor(private client: AttachmentClient, saved: SavedAttachment, private options: AttachOptions) {
    this.id = saved.id; this.cursor = BigInt(saved.cursor); this.handoff = saved.handoff;
  }

  get error(): Error | undefined { return this.failure; }

  async start(): Promise<void> {
    this.unsubscribe = await this.options.source?.subscribe(event => this.publish(event));
    if (this.failure) { this.unsubscribe?.(); this.unsubscribe = undefined; return; }
    this.schedule();
  }

  private schedule(): void {
    if (this.stopped || this.failure) return;
    this.timer = setTimeout(() => { void this.poll().catch(error => this.connectionLost(error)).finally(() => this.schedule()); }, 100);
  }

  connectionLost(error: Error): void {
    if (this.stopped || this.failure) return;
    this.failure = error;
    if (this.timer) clearTimeout(this.timer);
    this.unsubscribe?.(); this.unsubscribe = undefined;
    void this.client.peer.call('attachment.interrupt', { attachment_id: this.id, reason: error.message.slice(0, 2000) }).catch(() => {});
    try { this.options.onError?.(error); } catch { /* Preserve the original connection failure. */ }
  }

  async publish(event: HarnessEvent): Promise<void> {
    if (this.stopped || this.failure) throw this.failure ?? new Error('Attachment is detached');
    try { validate('HarnessEvent', event); }
    catch (error) { this.connectionLost(error as Error); throw error; }
    if (this.pending.size >= 32) {
      const error = new Error('Attachment input capacity exceeded; source coverage interrupted');
      this.connectionLost(error);
      throw error;
    }
    const operation = this.client.peer.call('attachment.events', { attachment_id: this.id, events: [event] }, AbortSignal.timeout(1000)).then(receipt => { this.cursor = BigInt(receipt.cursor) > this.cursor ? BigInt(receipt.cursor) : this.cursor; });
    this.pending.add(operation);
    try { await operation; }
    catch (error) {
      this.connectionLost(error as Error);
      throw error;
    } finally { this.pending.delete(operation); }
  }

  status(): Promise<AttachmentStatus> { return this.client.peer.call('attachment.status', { attachment_id: this.id }); }
  readRecord(id: string): Promise<RecordEnvelope> { return this.client.peer.call('attachment.record', { attachment_id: this.id, record_id: id }); }

  private poll(): Promise<void> {
    if (this.polling) return this.polling;
    this.polling = this.deliver().finally(() => { this.polling = undefined; });
    return this.polling;
  }

  private async deliver(): Promise<void> {
    if (this.stopped) return;
    const page = await this.client.peer.call('attachment.feedback', { attachment_id: this.id });
    for (const feedback of page.items) {
      if (this.stopped) break;
      try {
        const result = await bounded(Promise.resolve(this.options.onFeedback(feedback)), 30000, 'Feedback handler timed out; delivery outcome is unknown');
        await this.client.peer.call('attachment.ack', { attachment_id: this.id, feedback_id: feedback.id, outcome: result === 'rejected' ? 'rejected' : 'acknowledged', detail: 'Host accepted the feedback; this does not assert task success.' });
      } catch (error) {
        await this.client.peer.call('attachment.ack', { attachment_id: this.id, feedback_id: feedback.id, outcome: 'unknown', detail: 'Feedback handler failed or acknowledgement was lost.' }).catch(() => {});
        throw error;
      }
    }
  }

  async steer(feedback: AttachmentFeedback, message: string): Promise<void> {
    if (!this.options.steer || feedback.attachment_id !== this.id) throw new Error('Steering is not available for this feedback');
    await this.client.peer.call('attachment.steer', { attachment_id: this.id, feedback_id: feedback.id });
    await this.options.steer(message);
    await this.client.peer.call('attachment.ack', { attachment_id: this.id, feedback_id: feedback.id, outcome: 'acknowledged', detail: 'External harness queued steering; its effect is not yet observed.' });
  }

  async finish(timeoutMs = 120000): Promise<void> {
    await Promise.all(this.pending);
    if (this.failure) throw this.failure;
    await this.client.peer.call('attachment.complete', { attachment_id: this.id });
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (this.failure) throw this.failure;
      await this.poll();
      const status = await this.status();
      if (status.attachment.state !== 'active') throw new Error(status.attachment.last_error || 'Attachment interrupted');
      if (BigInt(status.attachment.cursor) >= this.cursor && status.queued_work === 0 && status.running_work === 0 && status.pending_feedback === 0) return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error('Attachment completion timed out; inspect status before continuing');
  }

  async repair(coordinator: WriteCoordinator, timeoutMs = 120000): Promise<void> {
    if (!this.options.coordinatedWrites) throw new Error('Coordinated writes are not enabled');
    await coordinator.handoff(async signal => {
      signal.throwIfAborted();
      this.handoff = await this.client.peer.call('attachment.repair', { attachment_id: this.id });
      for (;;) {
        signal.throwIfAborted();
        if (this.failure) throw this.failure;
        const status = await this.status();
        if (status.attachment.state !== 'active') throw new Error(status.attachment.last_error || 'Repair interrupted');
        if (status.running_work === 0 && status.queued_work === 0) break;
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      signal.throwIfAborted();
      await this.client.peer.call('attachment.release', { attachment_id: this.id, generation: this.handoff.generation });
      signal.throwIfAborted();
      this.handoff = undefined;
    }, timeoutMs);
  }

  async reconcileRepair(coordinator: WriteCoordinator, generation: string): Promise<void> {
    await coordinator.reconcile(async () => {
      await this.client.peer.call('attachment.release', { attachment_id: this.id, generation });
      this.handoff = undefined;
    });
  }

  async detach(): Promise<void> {
    if (this.detached) return;
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    this.unsubscribe?.(); this.unsubscribe = undefined;
    await Promise.allSettled(this.pending);
    await this.client.peer.call('attachment.detach', { attachment_id: this.id });
    this.detached = true;
    this.client.attachments.delete(this);
  }
}
