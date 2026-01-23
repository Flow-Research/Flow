import type {
  DistributedSearchResponse,
  SearchResult,
  NetworkStatus,
  SpaceStatus,
  PublishResponse,
} from '../../types/api';

// Search fixtures
export const mockLocalResult: SearchResult = {
  cid: 'bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi',
  score: 0.92,
  source: 'local',
  title: 'Test Document',
  snippet: 'This is a test document containing important information about the topic...',
  source_id: 'my-space',
};

export const mockNetworkResult: SearchResult = {
  cid: 'bafybeihxyz123456789abcdefghijklmnopqrstuvwxyz1234567890ab',
  score: 0.85,
  source: 'network',
  title: 'Network Result',
  snippet: 'Content from network peer with relevant information...',
  source_id: '12D3KooWabc123456789',
};

export const mockSearchResponse: DistributedSearchResponse = {
  success: true,
  query: 'test query',
  scope: 'all',
  results: [mockLocalResult, mockNetworkResult],
  local_count: 1,
  network_count: 1,
  peers_queried: 3,
  peers_responded: 2,
  total_found: 2,
  elapsed_ms: 245,
};

export const mockEmptySearchResponse: DistributedSearchResponse = {
  success: true,
  query: 'nonexistent query',
  scope: 'all',
  results: [],
  local_count: 0,
  network_count: 0,
  peers_queried: 3,
  peers_responded: 3,
  total_found: 0,
  elapsed_ms: 150,
};

export const mockLocalOnlySearchResponse: DistributedSearchResponse = {
  success: true,
  query: 'local search',
  scope: 'local',
  results: [mockLocalResult],
  local_count: 1,
  network_count: 0,
  peers_queried: 0,
  peers_responded: 0,
  total_found: 1,
  elapsed_ms: 50,
};

// Network fixtures
export const mockNetworkStatus: NetworkStatus = {
  connected: true,
  peer_count: 3,
  peers: [
    { id: '12D3KooWabc123456789abcdefghij', connected_at: '2026-01-22T10:00:00Z' },
    { id: '12D3KooWdef987654321zyxwvutsrq', connected_at: '2026-01-22T09:45:00Z' },
    { id: '12D3KooWghi456789012mnopqrstuv', connected_at: '2026-01-22T09:00:00Z' },
  ],
};

export const mockDisconnectedNetworkStatus: NetworkStatus = {
  connected: false,
  peer_count: 0,
  peers: [],
};

// Space status fixtures
export const mockSpaceStatusIdle: SpaceStatus = {
  indexing_in_progress: false,
  last_indexed: '2026-01-22T12:00:00Z',
  files_indexed: 42,
  chunks_stored: 256,
  files_failed: 0,
  last_error: null,
};

export const mockSpaceStatusIndexing: SpaceStatus = {
  indexing_in_progress: true,
  last_indexed: '2026-01-22T11:00:00Z',
  files_indexed: 20,
  chunks_stored: 128,
  files_failed: 0,
  last_error: null,
};

export const mockSpaceStatusWithError: SpaceStatus = {
  indexing_in_progress: false,
  last_indexed: '2026-01-22T10:00:00Z',
  files_indexed: 35,
  chunks_stored: 200,
  files_failed: 7,
  last_error: 'Failed to process file: document.pdf - Invalid format',
};

// Publish fixtures
export const mockPublishSuccess: PublishResponse = {
  success: true,
  cid: 'bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi',
  published_at: '2026-01-22T12:30:00Z',
};

export const mockPublishError: PublishResponse = {
  success: false,
  error: 'Space must be indexed before publishing',
};

export const mockUnpublishSuccess = {
  success: true,
  unpublished_at: '2026-01-22T13:00:00Z',
};
