export interface LeaseView { client_id: string; connection_id: string; epoch: number; remaining_ms: number }
export interface SessionInfo {
  session_id: string; project_id: string | null; label: string; initial_cwd: string;
  profile: string; agent_epoch: string; generation: number;
  state: 'starting' | 'running' | 'closing' | 'exited' | 'lost';
  cols: number; rows: number; output_seq: string;
  pid: number | null; process_created: string | null;
  program_path?: string | null; last_activity_at?: string | null;
  composer_allowed?: boolean; lease: LeaseView | null;
}
export interface SnapshotMeta {
  cols: number; rows: number; sequence: string; generation: number;
  agent_epoch: string; fidelity: string; warnings: string[]; bytes: number;
}
export interface HostInfo {
  agent_epoch: string; client_id: string; protocol: number; scopes: string[];
  snapshot_profile: string; capabilities: Record<string, boolean>;
}
export interface TerminalFrame { kind: number; sessionId: string; generation: number; sequence: bigint; payload: Uint8Array }
export class ProtocolError extends Error {
  constructor(public readonly code: string, message = code) { super(message); this.name = 'ProtocolError'; }
}
