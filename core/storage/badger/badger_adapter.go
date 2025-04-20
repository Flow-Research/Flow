package badger

import (
	"encoding/json"
	"fmt"
	"log"

	storage "flow-network/flow/core/storage"         // Import the interface package
	types "flow-network/flow/core/storage/types" // Import the types package

	"github.com/dgraph-io/badger/v4"
	"github.com/ipfs/go-cid"
	mh "github.com/multiformats/go-multihash"
)

// BadgerOptions holds configuration settings for BadgerDB.
// Exported to allow configuration from outside the package.
type BadgerOptions struct {
	Path string
}

// BadgerAdapter implements the StorageAdapter interface using BadgerDB.
type BadgerAdapter struct {
	db *badger.DB
}

// Compile-time check to ensure BadgerAdapter implements StorageAdapter
var _ storage.StorageAdapter = (*BadgerAdapter)(nil)

// NewBadgerAdapter creates a new BadgerDB storage adapter.
func NewBadgerAdapter(options BadgerOptions) (*BadgerAdapter, error) {
	opts := badger.DefaultOptions(options.Path)
	// Disable Badger's internal logger unless debugging
	opts.Logger = nil // Or replace with a proper logger if needed

	db, err := badger.Open(opts)
	if err != nil {
		return nil, fmt.Errorf("failed to open badger database at %s: %w", options.Path, err)
	}
	return &BadgerAdapter{db: db}, nil
}

// Close closes the BadgerDB database connection.
func (adapter *BadgerAdapter) Close() error {
	return adapter.db.Close()
}

// --- Key Generation Helpers ---

const (
	objectPrefix = "obj/"
	deltaPrefix  = "delta/"
	snapPrefix   = "snap/"
)

// objectKey generates the BadgerDB key for a FlowObject.
func objectKey(id types.ObjectId) []byte {
	return []byte(objectPrefix + id.String())
}

// deltaKey generates the BadgerDB key for a Delta, including sequence number.
func deltaKey(id types.ObjectId, sequence uint64) []byte {
	return []byte(fmt.Sprintf("%s%s/%020d", deltaPrefix, id.String(), sequence))
}

// snapshotKey generates the BadgerDB key for a Snapshot.
func snapshotKey(id types.SnapshotId) []byte {
	return []byte(snapPrefix + id.String())
}

// --- StorageAdapter Implementation ---

// CreateObject stores a new FlowObject and its initial state.
func (adapter *BadgerAdapter) CreateObject(objectType string, metadata map[string]interface{}, initialState []byte) (types.ObjectId, error) {
	obj := types.FlowObject{
		ObjectType: objectType,
		Metadata:   metadata,
		CRDTState:  initialState,
		Sequence:   0, // Initial sequence
	}

	// Generate deterministic CID based on initial content (consider adding type/metadata?)
	pref := cid.Prefix{
		Version:  1,
		Codec:    cid.Raw, // Using raw codec for the initial state bytes
		MhType:   mh.SHA2_256,
		MhLength: -1, // Default length
	}
	objCid, err := pref.Sum(initialState) // Use initial state for ID
	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to generate object CID: %w", err)
	}
	obj.ID = objCid

	objBytes, err := json.Marshal(obj)
	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to marshal object: %w", err)
	}

	err = adapter.db.Update(func(txn *badger.Txn) error {
		key := objectKey(obj.ID)
		return txn.Set(key, objBytes)
	})

	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to save object to badger: %w", err)
	}

	return obj.ID, nil
}

// LoadObject retrieves a FlowObject by its ID.
func (adapter *BadgerAdapter) LoadObject(id types.ObjectId) (*types.FlowObject, error) {
	var obj types.FlowObject
	err := adapter.db.View(func(txn *badger.Txn) error {
		key := objectKey(id)
		item, err := txn.Get(key)
		if err != nil {
			// Handle not found specifically if needed
			if err == badger.ErrKeyNotFound {
				return fmt.Errorf("object with id %s not found: %w", id.String(), err)
			}
			return fmt.Errorf("failed to get object %s: %w", id.String(), err)
		}

		err = item.Value(func(val []byte) error {
			return json.Unmarshal(val, &obj)
		})
		if err != nil {
			return fmt.Errorf("failed to unmarshal object %s: %w", id.String(), err)
		}
		return nil
	})

	if err != nil {
		return nil, err // Error already contains context
	}

	return &obj, nil
}

// ApplyDelta applies a change (Delta) to a specific FlowObject.
// It handles storing the delta and potentially updating the object's main state based on CRDT logic.
// TODO: Implement actual CRDT merge logic here.
// TODO: How to handle potential conflicts / concurrent deltas?
// TODO: How to efficiently update the main object state vs just storing deltas?
func (ba *BadgerAdapter) ApplyDelta(id types.ObjectId, delta *types.Delta) error {
	// Validate delta
	if delta == nil {
		return fmt.Errorf("cannot apply nil delta")
	}

	return ba.db.Update(func(txn *badger.Txn) error {
		// 1. Load the current object state
		key := objectKey(id)
		item, err := txn.Get(key)
		if err != nil {
			return fmt.Errorf("failed to get object %s for applying delta: %w", id.String(), err)
		}

		var currentObj types.FlowObject
		err = item.Value(func(val []byte) error {
			return json.Unmarshal(val, &currentObj)
		})
		if err != nil {
			return fmt.Errorf("failed to unmarshal object %s for delta: %w", id.String(), err)
		}

		// 2. Apply CRDT logic (placeholder: overwrite state)
		// In a real scenario, this would involve merging delta.Payload with currentObj.CRDTState
		// based on the specific CRDT rules.
		newSequence := currentObj.Sequence + 1
		newState := delta.Payload // Placeholder - just replace

		// 3. Store the delta itself, keyed by sequence number
		deltaBytes, err := json.Marshal(delta)
		if err != nil {
			return fmt.Errorf("failed to marshal delta for object %s: %w", id.String(), err)
		}
		dKey := deltaKey(id, newSequence)
		if err := txn.Set(dKey, deltaBytes); err != nil {
			return fmt.Errorf("failed to save delta %d for object %s: %w", newSequence, id.String(), err)
		}

		// 4. Update the object with the new state and sequence number
		currentObj.CRDTState = newState
		currentObj.Sequence = newSequence
		newObjBytes, err := json.Marshal(currentObj)
		if err != nil {
			return fmt.Errorf("failed to marshal updated object %s: %w", id.String(), err)
		}
		if err := txn.Set(key, newObjBytes); err != nil {
			return fmt.Errorf("failed to save updated object %s: %w", id.String(), err)
		}

		return nil
	})
}

// GetHistory retrieves the list of deltas applied to a FlowObject.
func (adapter *BadgerAdapter) GetHistory(id types.ObjectId) ([]types.Delta, error) {
	var history []types.Delta
	err := adapter.db.View(func(txn *badger.Txn) error {
		opts := badger.DefaultIteratorOptions
		opts.Prefix = []byte(fmt.Sprintf("%s%s/", deltaPrefix, id.String()))
		it := txn.NewIterator(opts)
		defer it.Close()

		for it.Rewind(); it.Valid(); it.Next() {
			item := it.Item()
			err := item.Value(func(val []byte) error {
				var delta types.Delta
				if err := json.Unmarshal(val, &delta); err != nil {
					// Log or handle unmarshaling error for a specific delta?
					log.Printf("Warning: Failed to unmarshal delta for key %s: %v", string(item.Key()), err)
					return nil // Skip this delta, continue with others
				}
				history = append(history, delta)
				return nil
			})
			if err != nil {
				return fmt.Errorf("failed reading value for delta key %s: %w", string(item.Key()), err)
			}
		}
		return nil
	})

	if err != nil {
		return nil, fmt.Errorf("failed to retrieve history for object %s: %w", id.String(), err)
	}

	return history, nil
}

// CreateSnapshot stores the current state of a FlowObject as a snapshot.
func (adapter *BadgerAdapter) CreateSnapshot(id types.ObjectId) (types.SnapshotId, error) {
	var snapshot types.FlowObject
	var snapshotCid types.SnapshotId

	err := adapter.db.Update(func(txn *badger.Txn) error {
		// 1. Load the current object state
		objKey := objectKey(id)
		item, err := txn.Get(objKey)
		if err != nil {
			return fmt.Errorf("failed to get object %s for snapshotting: %w", id.String(), err)
		}

		var objBytes []byte
		err = item.Value(func(val []byte) error {
			objBytes = append([]byte{}, val...) // Important: Copy the value!
			return json.Unmarshal(val, &snapshot) // Unmarshal to get fields if needed
		})
		if err != nil {
			return fmt.Errorf("failed to read/unmarshal object %s for snapshot: %w", id.String(), err)
		}

		// 2. Generate snapshot ID (CID of the *entire* marshaled object state at this point)
		pref := cid.Prefix{
			Version:  1,
			Codec:    cid.DagJSON, // Use DagJSON as we're storing the marshaled JSON
			MhType:   mh.SHA2_256,
			MhLength: -1,
		}
		snapCid, err := pref.Sum(objBytes)
		if err != nil {
			return fmt.Errorf("failed to generate snapshot CID for object %s: %w", id.String(), err)
		}
		snapshotCid = snapCid // Assign to outer scope variable

		// 3. Store the snapshot data using the snapshot key
		snapKey := snapshotKey(snapshotCid)
		if err := txn.Set(snapKey, objBytes); err != nil {
			return fmt.Errorf("failed to save snapshot %s for object %s: %w", snapshotCid.String(), id.String(), err)
		}

		return nil
	})

	if err != nil {
		return types.UndefSnapshotId, err
	}

	return snapshotCid, nil
}

// LoadSnapshot retrieves a FlowObject snapshot by its Snapshot ID.
func (adapter *BadgerAdapter) LoadSnapshot(id types.SnapshotId) (*types.FlowObject, error) {
	var obj types.FlowObject
	err := adapter.db.View(func(txn *badger.Txn) error {
		key := snapshotKey(id)
		item, err := txn.Get(key)
		if err != nil {
			if err == badger.ErrKeyNotFound {
				return fmt.Errorf("snapshot with id %s not found: %w", id.String(), err)
			}
			return fmt.Errorf("failed to get snapshot %s: %w", id.String(), err)
		}

		err = item.Value(func(val []byte) error {
			return json.Unmarshal(val, &obj)
		})
		if err != nil {
			return fmt.Errorf("failed to unmarshal snapshot %s: %w", id.String(), err)
		}
		return nil
	})

	if err != nil {
		return nil, err
	}

	return &obj, nil
}
