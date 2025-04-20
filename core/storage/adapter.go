package storage

import (
	types "flow-network/flow/core/storage/types"
)

// StorageAdapter defines the interface for interacting with the underlying storage layer.
// This abstracts the specific storage implementation (e.g., BadgerDB, IPFS blockstore).
type StorageAdapter interface {
	// CreateObject initializes a new object with its initial state and returns its unique ID.
	CreateObject(objectType string, metadata map[string]interface{}, initialCRDTState []byte) (types.ObjectId, error)

	// LoadObject retrieves the current state of an object by its ID.
	LoadObject(id types.ObjectId) (*types.FlowObject, error)

	// ApplyDelta applies a delta (change) to an object.
	// This should handle the CRDT merge logic internally.
	ApplyDelta(id types.ObjectId, delta *types.Delta) error

	// GetHistory retrieves the sequence of deltas applied to an object.
	GetHistory(id types.ObjectId) ([]types.Delta, error)

	// CreateSnapshot stores the current state of an object as an immutable snapshot.
	// Returns the unique ID (e.g., CID) of the snapshot.
	CreateSnapshot(id types.ObjectId) (types.SnapshotId, error)

	// LoadSnapshot retrieves the object state from a specific snapshot ID.
	LoadSnapshot(id types.SnapshotId) (*types.FlowObject, error)

	// Close releases any resources held by the storage adapter.
	Close() error
}
