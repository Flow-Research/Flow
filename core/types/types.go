package types

import (
	"github.com/ipfs/go-cid"
)

// ObjectId represents the unique identifier for a Flow object, based on IPLD's CID.
// Using a type alias for now, but could become a struct wrapper if more methods are needed.
type ObjectId = cid.Cid

// SnapshotId represents the unique identifier for a snapshot of a Flow object's state.
// Also based on IPLD's CID.
type SnapshotId = cid.Cid

// UndefObjectId represents an undefined or invalid ObjectId.
var UndefObjectId = cid.Undef

// UndefSnapshotId represents an undefined or invalid SnapshotId.
var UndefSnapshotId = cid.Undef

// FlowObject represents a generic data object within the Flow system.
// It encapsulates the object's identity, type, metadata, and its current CRDT state.
type FlowObject struct {
	ID         ObjectId               `json:"id"`         // Unique identifier (CID) of the object's initial state or definition
	ObjectType string                 `json:"object_type"` // String identifier for the type of object (e.g., "document", "task", "schema:person")
	Metadata   map[string]interface{} `json:",omitempty"` // Optional user-defined metadata
	State      []byte                 `json:"state"`      // The current serialized state of the underlying CRDT
	Sequence   uint64                 `json:"sequence"`   // Monotonically increasing sequence number for ordering deltas
}
