const API_BASE = 'http://localhost:8080/api/v1';

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const token = localStorage.getItem('token');

  const headers: HeadersInit = {
    'Content-Type': 'application/json',
    ...options.headers,
  };

  if (token) {
    (headers as Record<string, string>)['Authorization'] = `Bearer ${token}`;
  }

  const response = await fetch(`${API_BASE}${endpoint}`, {
    ...options,
    headers,
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new ApiError(response.status, errorText || response.statusText);
  }

  return response.json();
}

export interface AuthStartResponse {
  challenge: PublicKeyCredentialCreationOptions | PublicKeyCredentialRequestOptions;
  challenge_id: string;
}

export interface AuthFinishResponse {
  verified: boolean;
  token?: string;
  did?: string;
  message?: string;
}

export interface Space {
  id: number;
  key: string;
  name: string | null;
  location: string;
  time_created: string;
}

export interface SpaceStatus {
  indexing_in_progress: boolean;
  last_indexed: string | null;
  files_indexed: number;
  chunks_stored: number;
  files_failed: number;
  last_error: string | null;
}

export interface QueryResponse {
  status: string;
  response: string;
}

export interface EntityEdge {
  id: number;
  edge_type: string;
  source_id?: number;
  target_id?: number;
  direction: 'incoming' | 'outgoing';
}

export interface Entity {
  id: number;
  cid: string;
  name: string;
  entity_type: string;
  properties: Record<string, unknown>;
  created_at: string;
  edges?: EntityEdge[];
}

export const api = {
  auth: {
    startRegistration: () =>
      request<AuthStartResponse>('/webauthn/start_registration'),

    finishRegistration: (challengeId: string, credential: unknown) =>
      request<AuthFinishResponse>('/webauthn/finish_registration', {
        method: 'POST',
        body: JSON.stringify({ challenge_id: challengeId, credential }),
      }),

    startAuthentication: () =>
      request<AuthStartResponse>('/webauthn/start_authentication', {
        method: 'POST',
        body: '{}',
      }),

    finishAuthentication: (challengeId: string, credential: unknown) =>
      request<AuthFinishResponse>('/webauthn/finish_authentication', {
        method: 'POST',
        body: JSON.stringify({ challenge_id: challengeId, credential }),
      }),
  },

  spaces: {
    list: () => request<Space[]>('/spaces'),

    create: (dir: string) =>
      request<{ status: string }>('/spaces', {
        method: 'POST',
        body: JSON.stringify({ dir }),
      }),

    get: (key: string) => request<Space>(`/spaces/${key}`),

    delete: (key: string) =>
      request<{ status: string }>(`/spaces/${key}`, { method: 'DELETE' }),

    getStatus: (key: string) => request<SpaceStatus>(`/spaces/${key}/status`),

    reindex: (key: string) =>
      request<{ status: string }>(`/spaces/${key}/reindex`, { method: 'POST' }),
  },

  query: {
    search: (spaceKey: string, query: string) =>
      request<QueryResponse>(`/spaces/search?space_key=${encodeURIComponent(spaceKey)}&query=${encodeURIComponent(query)}`),
  },

  entities: {
    list: (spaceKey: string) => request<Entity[]>(`/spaces/${spaceKey}/entities`),

    get: (spaceKey: string, id: number) =>
      request<Entity>(`/spaces/${spaceKey}/entities/${id}`),
  },

  health: {
    check: () => request<{ status: string; timestamp: string }>('/health'),
  },
};
