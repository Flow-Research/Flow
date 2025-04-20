package badger

import (
	"context"
	"encoding/json"
	"fmt"

	storage "flow-network/flow/core/storage"     // Interface
	types "flow-network/flow/core/storage/types" // Types

	badgerds "github.com/ipfs/go-ds-badger" // Use go-ds-badger
	datastore "github.com/ipfs/go-datastore"
	query "github.com/ipfs/go-datastore/query"
	logging "github.com/ipfs/go-log/v2"
	cid "github.com/ipfs/go-cid"
	mh "github.com/multiformats/go-multihash"
)

var log = logging.Logger("flow/storage/badger") // Setup logger

// --- Constants for Key Prefixes ---
const (
	metadataPrefix = "/meta/"
	statePrefix    = "/state/"
	deltaLogPrefix = "/dlog/"
	snapshotPrefix = "/snap/"
)

// --- Metadata Structure ---

// objectMetadata holds the non-state information about a FlowObject.
type objectMetadata struct {
	ID         types.ObjectId         `json:"id"`
	ObjectType string                 `json:"object_type"`
	Metadata   map[string]interface{} `json:"metadata"`
	Sequence   uint64                 `json:"sequence"`
}

// --- Adapter Structure ---

// BadgerOptions holds configuration settings for BadgerDB datastore.
// Exported to allow configuration from outside the package.
type BadgerOptions struct {
	Path string
}

// BadgerAdapter implements the StorageAdapter interface using go-ds-badger.
type BadgerAdapter struct {
	ds datastore.Batching // Use the datastore interface
}

// Compile-time check to ensure BadgerAdapter implements StorageAdapter
var _ storage.StorageAdapter = (*BadgerAdapter)(nil)

// NewBadgerAdapter creates a new BadgerDB storage adapter using go-ds-badger.
func NewBadgerAdapter(options BadgerOptions) (*BadgerAdapter, error) {
	// Configure Badger datastore options (can customize more if needed)
	// See go-ds-badger documentation for available options
	// Using DefaultOptions here for simplicity.
	dsOpts := badgerds.DefaultOptions
	ds, err := badgerds.NewDatastore(options.Path, &dsOpts)
	if err != nil {
		return nil, fmt.Errorf("failed to create badger datastore at %s: %w", options.Path, err)
	}

	log.Infof("Badger datastore opened at %s", options.Path)
	return &BadgerAdapter{ds: ds}, nil
}

// Close closes the underlying Badger datastore.
func (adapter *BadgerAdapter) Close() error {
	if adapter.ds == nil {
		return nil // Already closed or never opened
	}
	err := adapter.ds.Close()
	adapter.ds = nil // Prevent further use
	if err != nil {
		log.Errorf("Error closing badger datastore: %v", err)
		return fmt.Errorf("failed to close badger datastore: %w", err)
	}
	log.Info("Badger datastore closed")
	return nil
}

// --- Key Helper Functions (using datastore.Key) ---

func objectMetaKey(id types.ObjectId) datastore.Key {
	return datastore.NewKey(metadataPrefix + id.String())
}

func objectStateKey(id types.ObjectId) datastore.Key {
	return datastore.NewKey(statePrefix + id.String())
}

func deltaLogKey(id types.ObjectId, sequence uint64) datastore.Key {
	// Ensure sequence is padded for correct lexicographical sorting
	return datastore.NewKey(fmt.Sprintf("%s%s/%020d", deltaLogPrefix, id.String(), sequence))
}

func deltaLogPrefixKey(id types.ObjectId) datastore.Key {
	return datastore.NewKey(fmt.Sprintf("%s%s/", deltaLogPrefix, id.String()))
}

func snapshotMetaKey(id types.SnapshotId) datastore.Key {
	return datastore.NewKey(snapshotPrefix + id.String() + "/meta")
}

func snapshotStateKey(id types.SnapshotId) datastore.Key {
	return datastore.NewKey(snapshotPrefix + id.String() + "/state")
}

// --- StorageAdapter Implementation (using Datastore) ---

// CreateObject initializes a new FlowObject, storing its metadata and initial state.
func (adapter *BadgerAdapter) CreateObject(objectType string, metadata map[string]interface{}, initialState []byte) (types.ObjectId, error) {
	ctx := context.Background()

	// Generate object ID based on type and initial state
	pref := cid.Prefix{MhType: mh.SHA2_256, MhLength: -1, Version: 1, Codec: cid.Raw}
	hashBytes := append([]byte(objectType), initialState...)
	hashedCid, err := pref.Sum(hashBytes)
	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to generate object CID: %w", err)
	}
	objId := types.ObjectId(hashedCid)

	// Prepare metadata
	metaData := objectMetadata{
		ID:         objId,
		ObjectType: objectType,
		Metadata:   metadata,
		Sequence:   0, // Initial sequence
	}
	metaBytes, err := json.Marshal(metaData)
	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to marshal object metadata: %w", err)
	}

	// Use a batch write for atomicity (metadata + initial state)
	batch, err := adapter.ds.Batch(ctx)
	if err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to start batch for create object: %w", err)
	}

	metaKey := objectMetaKey(objId)
	if err = batch.Put(ctx, metaKey, metaBytes); err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to put metadata in batch: %w", err)
	}

	stateKey := objectStateKey(objId)
	if err = batch.Put(ctx, stateKey, initialState); err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to put initial state in batch: %w", err)
	}

	if err = batch.Commit(ctx); err != nil {
		return types.UndefObjectId, fmt.Errorf("failed to commit create object batch: %w", err)
	}

	log.Debugf("Created object %s of type %s", objId, objectType)
	return objId, nil
}

// LoadObject retrieves a FlowObject by its ID, including its latest state.
func (adapter *BadgerAdapter) LoadObject(id types.ObjectId) (*types.FlowObject, error) {
	ctx := context.Background()
	metaKey := objectMetaKey(id)
	stateKey := objectStateKey(id)

	// Load metadata
	metaBytes, err := adapter.ds.Get(ctx, metaKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			return nil, fmt.Errorf("object metadata not found for ID %s: %w", id, datastore.ErrNotFound)
		}
		return nil, fmt.Errorf("failed to load object metadata: %w", err)
	}
	var metaData objectMetadata
	if err := json.Unmarshal(metaBytes, &metaData); err != nil {
		return nil, fmt.Errorf("failed to unmarshal object metadata: %w", err)
	}

	// Load state
	stateBytes, err := adapter.ds.Get(ctx, stateKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			// Should not happen if metadata exists, indicates inconsistency
			log.Errorf("State missing for object %s where metadata exists", id)
			return nil, fmt.Errorf("state data inconsistency for ID %s: %w", id, datastore.ErrNotFound)
		}
		return nil, fmt.Errorf("failed to load object state: %w", err)
	}

	// Populate the FlowObject
	flowObj := &types.FlowObject{
		ID:         metaData.ID,
		ObjectType: metaData.ObjectType,
		Metadata:   metaData.Metadata,
		Sequence:   metaData.Sequence,
		State:      stateBytes, // Use the generic 'State' field
	}

	log.Debugf("Loaded object %s", flowObj.ID)
	return flowObj, nil
}

// ApplyDelta updates an object's state, increments its sequence, and logs the delta.
func (adapter *BadgerAdapter) ApplyDelta(id types.ObjectId, delta *types.Delta) error {
	ctx := context.Background()
	metaKey := objectMetaKey(id)
	stateKey := objectStateKey(id)

	// 1. Load current metadata to get the current sequence number
	metaBytes, err := adapter.ds.Get(ctx, metaKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			return fmt.Errorf("cannot apply delta, object not found for ID %s: %w", id, datastore.ErrNotFound)
		}
		return fmt.Errorf("failed to load object metadata for delta: %w", err)
	}
	var metaData objectMetadata
	if err := json.Unmarshal(metaBytes, &metaData); err != nil {
		return fmt.Errorf("failed to unmarshal object metadata for delta: %w", err)
	}

	// 2. Prepare updated metadata and delta log entry
	newSequence := metaData.Sequence + 1
	metaData.Sequence = newSequence // Increment sequence
	updatedMetaBytes, err := json.Marshal(metaData)
	if err != nil {
		return fmt.Errorf("failed to marshal updated metadata: %w", err)
	}

	historyKey := deltaLogKey(id, newSequence)
	deltaBytes, err := json.Marshal(delta)
	if err != nil {
		return fmt.Errorf("failed to marshal delta for history log: %w", err)
	}

	// 3. Use a batch write for atomicity (metadata + state + history log)
	batch, err := adapter.ds.Batch(ctx)
	if err != nil {
		return fmt.Errorf("failed to start batch for apply delta: %w", err)
	}

	// Put updated metadata
	if err = batch.Put(ctx, metaKey, updatedMetaBytes); err != nil {
		return fmt.Errorf("failed to put updated metadata in batch: %w", err)
	}

	// Put new state (overwrite with delta payload)
	if err = batch.Put(ctx, stateKey, delta.Payload); err != nil {
		return fmt.Errorf("failed to put new state in batch: %w", err)
	}

	// Put delta log entry
	if err = batch.Put(ctx, historyKey, deltaBytes); err != nil {
		return fmt.Errorf("failed to put delta log entry in batch: %w", err)
	}

	// Commit the atomic batch
	if err = batch.Commit(ctx); err != nil {
		return fmt.Errorf("failed to commit apply delta batch: %w", err)
	}

	log.Debugf("Applied delta to object %s, new sequence %d", id, newSequence)
	return nil
}

// GetHistory retrieves the sequence of deltas applied to an object.
func (adapter *BadgerAdapter) GetHistory(id types.ObjectId) ([]types.Delta, error) {
	ctx := context.Background()
	prefix := deltaLogPrefixKey(id)

	q := query.Query{
		Prefix: prefix.String(),
		Orders: []query.Order{query.OrderByKey{}}, // Order by sequence number (encoded in key)
	}

	results, err := adapter.ds.Query(ctx, q)
	if err != nil {
		return nil, fmt.Errorf("failed to query delta history for ID %s: %w", id, err)
	}
	defer results.Close()

	var history []types.Delta
	for result := range results.Next() {
		if result.Error != nil {
			return nil, fmt.Errorf("error iterating delta history results for ID %s: %w", id, result.Error)
		}

		var delta types.Delta
		if err := json.Unmarshal(result.Value, &delta); err != nil {
			log.Errorf("Failed to unmarshal delta from history log (ID: %s, Key: %s): %v", id, result.Key, err)
			continue // Skip corrupted entry
		}
		history = append(history, delta)
	}

	log.Debugf("Retrieved %d history entries for object %s", len(history), id)
	return history, nil
}

// CreateSnapshot stores the current state of an object.
func (adapter *BadgerAdapter) CreateSnapshot(id types.ObjectId) (types.SnapshotId, error) {
	ctx := context.Background()
	objMetaKey := objectMetaKey(id)
	objStateKey := objectStateKey(id)

	// 1. Load current metadata
	metaBytes, err := adapter.ds.Get(ctx, objMetaKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			return types.UndefSnapshotId, fmt.Errorf("cannot snapshot, object not found %s: %w", id, datastore.ErrNotFound)
		}
		return types.UndefSnapshotId, fmt.Errorf("failed to load object metadata for snapshot: %w", err)
	}
	var currentMeta objectMetadata
	if err := json.Unmarshal(metaBytes, &currentMeta); err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to unmarshal object metadata for snapshot: %w", err)
	}

	// 2. Load current state
	currentStateBytes, err := adapter.ds.Get(ctx, objStateKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			log.Errorf("State missing for object %s during snapshot where metadata exists", id)
			return types.UndefSnapshotId, fmt.Errorf("state data inconsistency for snapshot on ID %s", id)
		}
		return types.UndefSnapshotId, fmt.Errorf("failed to load object state for snapshot: %w", err)
	}

	// 3. Prepare snapshot metadata (using the same struct)
	snapshotMeta := currentMeta // Copy current metadata
	snapMetaBytes, err := json.Marshal(snapshotMeta)
	if err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to marshal snapshot metadata: %w", err)
	}

	// 4. Generate snapshot ID (hash of meta + state)
	hashInput := append(snapMetaBytes, currentStateBytes...)
	pref := cid.Prefix{MhType: mh.SHA2_256, MhLength: -1, Version: 1, Codec: cid.Raw}
	snapshotCid, err := pref.Sum(hashInput)
	if err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to generate snapshot CID: %w", err)
	}
	snapId := types.SnapshotId(snapshotCid)

	// 5. Store snapshot atomically (metadata + state)
	snapMetaKey := snapshotMetaKey(snapId)
	snapStateKey := snapshotStateKey(snapId)

	batch, err := adapter.ds.Batch(ctx)
	if err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to start batch for create snapshot: %w", err)
	}

	if err = batch.Put(ctx, snapMetaKey, snapMetaBytes); err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to put snapshot metadata in batch: %w", err)
	}

	if err = batch.Put(ctx, snapStateKey, currentStateBytes); err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to put snapshot state in batch: %w", err)
	}

	if err = batch.Commit(ctx); err != nil {
		return types.UndefSnapshotId, fmt.Errorf("failed to commit create snapshot batch: %w", err)
	}

	log.Debugf("Created snapshot %s for object %s", snapId, id)
	return snapId, nil
}

// LoadSnapshot retrieves a previously created object snapshot.
func (adapter *BadgerAdapter) LoadSnapshot(id types.SnapshotId) (*types.FlowObject, error) {
	ctx := context.Background()
	snapMetaKey := snapshotMetaKey(id)
	snapStateKey := snapshotStateKey(id)

	// Load snapshot metadata
	metaBytes, err := adapter.ds.Get(ctx, snapMetaKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			return nil, fmt.Errorf("snapshot metadata not found for ID %s: %w", id, datastore.ErrNotFound)
		}
		return nil, fmt.Errorf("failed to load snapshot metadata: %w", err)
	}
	var metaData objectMetadata
	if err := json.Unmarshal(metaBytes, &metaData); err != nil {
		return nil, fmt.Errorf("failed to unmarshal snapshot metadata: %w", err)
	}

	// Load snapshot state
	stateBytes, err := adapter.ds.Get(ctx, snapStateKey)
	if err != nil {
		if err == datastore.ErrNotFound {
			log.Errorf("Snapshot state missing for ID %s where metadata exists", id)
			return nil, fmt.Errorf("snapshot state inconsistency for ID %s: %w", id, datastore.ErrNotFound)
		}
		return nil, fmt.Errorf("failed to load snapshot state: %w", err)
	}

	// Reconstruct the FlowObject
	flowObj := &types.FlowObject{
		ID:         metaData.ID,       // ID of the original object from snapshot meta
		ObjectType: metaData.ObjectType,
		Metadata:   metaData.Metadata,
		Sequence:   metaData.Sequence, // Sequence at the time of snapshot
		State:      stateBytes,     // State from the snapshot
	}

	log.Debugf("Loaded snapshot %s (Original Object: %s)", id, metaData.ID)
	return flowObj, nil
}
