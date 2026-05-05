// SPDX-License-Identifier: AGPL-3.0-only
import type { RuntimeJobEvent } from '$lib/types';

export type HostWorkflowResultKind = 'workspace' | 'shell' | 'export' | 'none';

export interface HostWorkflowPayload<T> {
  resultKind: HostWorkflowResultKind | 'error';
  data?: T;
  workspaceChanged?: boolean;
  nestedJobIds?: string[];
  message?: string;
  error?: {
    message?: string;
    code?: string;
    jobIds?: string[];
    nestedJobIds?: string[];
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPayload<T>(event: RuntimeJobEvent): HostWorkflowPayload<T> {
  return event.payload as unknown as HostWorkflowPayload<T>;
}

export function hostWorkflowData<T>(
  event: RuntimeJobEvent,
  expectedKind: HostWorkflowResultKind
): T {
  const payload = readPayload<T>(event);
  if (payload.resultKind !== expectedKind) {
    throw new Error(
      `Unexpected completion payload for ${event.commandName}: expected ${expectedKind}, received ${String(payload.resultKind)}.`
    );
  }
  if (!('data' in payload)) {
    throw new Error(`Completion payload for ${event.commandName} did not include data.`);
  }
  return payload.data as T;
}

export function hostWorkflowErrorMessage(event: RuntimeJobEvent): string {
  const payload = event.payload;
  if (typeof payload.message === 'string' && payload.message.trim().length > 0) {
    return payload.message;
  }
  if (isRecord(payload.error) && typeof payload.error.message === 'string') {
    return payload.error.message;
  }
  return event.eventType;
}

export function hostWorkflowChangedWorkspace(event: RuntimeJobEvent): boolean {
  return event.payload.workspaceChanged === true;
}
