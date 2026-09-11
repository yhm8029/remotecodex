// packages/terminal-client/src/perf.ts

export type PerfStage =
  | 'client_input_enqueue'
  | 'client_output_parse_apply'
  | 'agent_input_write';

export interface PerfEvent {
  stage: PerfStage;
  session_id: string;
  elapsed_ms: number;
  bytes?: number;
  sequence?: string;
}

export type PerfSink = (event: PerfEvent) => void;

const STAGES: ReadonlySet<PerfStage> = new Set<PerfStage>([
  'client_input_enqueue',
  'client_output_parse_apply',
  'agent_input_write',
]);

function isValidEvent(event: PerfEvent): boolean {
  if (!event || typeof event !== 'object') return false;
  if (!STAGES.has(event.stage)) return false;
  if (typeof event.session_id !== 'string') return false;
  if (typeof event.elapsed_ms !== 'number') return false;
  if (!Number.isFinite(event.elapsed_ms)) return false;
  if (event.elapsed_ms < 0) return false;
  if (event.bytes !== undefined && (typeof event.bytes !== 'number' || !Number.isFinite(event.bytes) || event.bytes < 0)) {
    return false;
  }
  if (event.sequence !== undefined && typeof event.sequence !== 'string') {
    return false;
  }
  return true;
}

export function emitPerf(sink: PerfSink | undefined, event: PerfEvent): void {
  if (sink === undefined) return;
  if (!isValidEvent(event)) return;
  try {
    sink(event);
  } catch {
    // swallow callback exceptions
  }
}

export class PerfBuffer {
  readonly capacity: number;
  events: PerfEvent[] = [];
  dropped: number = 0;
  readonly record: PerfSink;

  constructor(capacity: number = 100000) {
    if (!Number.isInteger(capacity) || capacity < 1 || capacity > 1000000) {
      throw new RangeError('PerfBuffer capacity must be an integer between 1 and 1000000');
    }
    this.capacity = capacity;
    this.record = (event: PerfEvent): void => {
      if (!isValidEvent(event)) return;
      if (this.events.length < this.capacity) {
        this.events.push({ ...event });
      } else {
        this.dropped++;
      }
    };
  }

  clear(): void {
    this.events = [];
    this.dropped = 0;
  }
}
