/**
 * Shared API types for Flow Network frontend.
 * These types mirror the backend REST API responses.
 */

// ============================================================================
// Distributed Search Types
// ============================================================================

/** Search scope determines which sources to query */
export type SearchScope = 'local' | 'network' | 'all';

/** Request parameters for distributed search */
export interface DistributedSearchRequest {
  /** The search query text */
  query: string;
  /** Search scope: local spaces, network peers, or both */
  scope: SearchScope;
  /** Maximum results to return (1-100, default 20) */
  limit?: number;
  /** Pagination offset: skip first N results */
  offset?: number;
}

/** A single search result from local or network sources */
export interface SearchResult {
  /** Content identifier (CID) */
  cid: string;
  /** Relevance score (0-1) */
  score: number;
  /** Result source: local space or network peer */
  source: 'local' | 'network';
  /** Content title if available */
  title?: string;
  /** Content snippet/preview */
  snippet?: string;
  /** Source identifier (space key or peer ID) */
  source_id?: string;
}

/** Response from distributed search endpoint */
export interface DistributedSearchResponse {
  /** Whether the search was successful */
  success: boolean;
  /** The original query */
  query: string;
  /** Search scope used */
  scope: string;
  /** Search results sorted by relevance */
  results: SearchResult[];
  /** Number of results from local sources */
  local_count: number;
  /** Number of results from network sources */
  network_count: number;
  /** Number of peers queried (for network searches) */
  peers_queried: number;
  /** Number of peers that responded */
  peers_responded: number;
  /** Total results found (may be more than returned) */
  total_found: number;
  /** Search time in milliseconds */
  elapsed_ms: number;
}

/** Search system health response */
export interface SearchHealthResponse {
  status: string;
  cache: {
    entries: number;
    hits: number;
    misses: number;
  };
}

// ============================================================================
// Space & Publishing Types
// ============================================================================

/** Space data model */
export interface Space {
  id: number;
  key: string;
  name: string | null;
  location: string;
  time_created: string;
  /** Whether space is published to network */
  is_published?: boolean;
  /** Timestamp when space was published */
  published_at?: string;
}

/** Space indexing status */
export interface SpaceStatus {
  /** Whether indexing is currently in progress */
  indexing_in_progress: boolean;
  /** Last successful indexing timestamp */
  last_indexed: string | null;
  /** Number of files successfully indexed */
  files_indexed: number;
  /** Number of chunks stored in vector DB */
  chunks_stored: number;
  /** Number of files that failed to index */
  files_failed: number;
  /** Last indexing error message */
  last_error: string | null;
}

/** Response from publish operation */
export interface PublishResponse {
  success: boolean;
  /** Content identifier for published content */
  cid?: string;
  /** Timestamp when published */
  published_at?: string;
  /** Error message if failed */
  error?: string;
}

/** Response from unpublish operation */
export interface UnpublishResponse {
  success: boolean;
}

// ============================================================================
// Network Types
// ============================================================================

/** Information about a connected peer */
export interface PeerInfo {
  /** Peer ID (libp2p peer ID) */
  id: string;
  /** Timestamp when peer connected */
  connected_at: string;
}

/** Network connection status */
export interface NetworkStatus {
  /** Whether connected to the network */
  connected: boolean;
  /** Number of connected peers */
  peer_count: number;
  /** List of connected peers */
  peers: PeerInfo[];
}

// ============================================================================
// Error Types
// ============================================================================

/** Standardized API error response */
export interface ApiErrorResponse {
  error: {
    code: string;
    message: string;
  };
}
