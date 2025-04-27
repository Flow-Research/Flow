package storage

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"

	"flow-network/flow/core/types"

	datastore "github.com/ipfs/go-datastore"
	crdt "github.com/ipfs/go-ds-crdt"
	logging "github.com/ipfs/go-log/v2"
)

var opLog = logging.Logger("core/storage/operations")

// GenerateKeyFromObjectID creates a datastore key from a FlowObject's ObjectID (CID).
// It uses the default datastore key transformation for CIDs.
func GenerateKeyFromObjectID(id types.ObjectId) datastore.Key {
	if id == types.UndefObjectId {
		// Return an empty key or handle as an error? Returning empty for now.
		// Consider logging a warning here.
		opLog.Warnf("Attempted to generate key for undefined ObjectId")
		return datastore.Key{}
	}
	// CIDs can be directly converted to datastore Keys
	return datastore.NewKey(id.String())
}

// flowObjectMetadata holds the non-CRDT state fields of a FlowObject.
// This is serialized and stored in the base datastore.
type flowObjectMetadata struct {
	ID         types.ObjectId         `json:"id"`
	ObjectType string                 `json:"objectType"`
	Metadata   map[string]interface{} `json:"metadata"`
	Sequence   uint64                 `json:"sequence"`
}

// SaveFlowObject persists a FlowObject by storing its state in the CRDT store
// and its metadata in the base store.
func SaveFlowObject(ctx context.Context, crdtStore *crdt.Datastore, baseStore datastore.Datastore, obj *types.FlowObject) error {
	if obj == nil {
		return fmt.Errorf("cannot save nil FlowObject")
	}
	if obj.ID == types.UndefObjectId {
		return fmt.Errorf("cannot save FlowObject with undefined ID")
	}

	key := GenerateKeyFromObjectID(obj.ID)
	if key.String() == "" {
		return fmt.Errorf("generated empty key for FlowObject ID %s", obj.ID)
	}
	opLog.Debugf("Saving FlowObject with key: %s", key)

	// 1. Prepare and save metadata to baseStore
	meta := flowObjectMetadata{
		ID:         obj.ID,
		ObjectType: obj.ObjectType,
		Metadata:   obj.Metadata,
		Sequence:   obj.Sequence,
	}

	metaBytes, err := json.Marshal(meta)
	if err != nil {
		return fmt.Errorf("failed to marshal metadata for FlowObject %s: %w", obj.ID, err)
	}

	err = baseStore.Put(ctx, key, metaBytes)
	if err != nil {
		return fmt.Errorf("failed to put metadata for FlowObject %s into base store: %w", obj.ID, err)
	}

	// 2. Save state bytes to crdtStore ONLY if state is not nil
	var stateLen int
	if obj.State != nil {
		err = crdtStore.Put(ctx, key, obj.State)
		if err != nil {
			// Attempt cleanup? For now, just error.
			// Consider deleting the metadata key from baseStore here for consistency.
			return fmt.Errorf("failed to put state for FlowObject %s into CRDT store: %w", obj.ID, err)
		}
		stateLen = len(obj.State)
	}

	opLog.Infof("Successfully saved FlowObject %s (State: %d bytes, Meta: %d bytes)", obj.ID, stateLen, len(metaBytes))
	return nil
}

// LoadFlowObject retrieves a FlowObject by loading its state from the CRDT store
// and its metadata from the base store.
func LoadFlowObject(ctx context.Context, crdtStore *crdt.Datastore, baseStore datastore.Datastore, id types.ObjectId) (*types.FlowObject, error) {
	if id == types.UndefObjectId {
		return nil, fmt.Errorf("cannot load FlowObject with undefined ID")
	}

	key := GenerateKeyFromObjectID(id)
	if key.String() == "" {
		return nil, fmt.Errorf("generated empty key for FlowObject ID %s", id)
	}
	opLog.Debugf("Loading FlowObject with key: %s", key)

	// 1. Load metadata from baseStore
	metaBytes, err := baseStore.Get(ctx, key)
	metaNotFound := errors.Is(err, datastore.ErrNotFound)
	if err != nil && !metaNotFound {
		return nil, fmt.Errorf("failed to get metadata for FlowObject %s from base store: %w", id, err)
	}

	// 2. Load state from crdtStore
	var stateNotFound bool
	stateBytes, err := crdtStore.Get(ctx, key)
	if err != nil {
		if errors.Is(err, datastore.ErrNotFound) {
			stateNotFound = true
			stateBytes = nil // Ensure stateBytes is nil if truly not found
		} else {
			// Handle other potential errors during Get
			return nil, fmt.Errorf("failed to get state for FlowObject %s from CRDT store: %w", id, err)
		}
	} else if len(stateBytes) == 0 {
		// Explicitly check if Get succeeded but returned an empty slice
		opLog.Debugf("FlowObject %s state loaded as empty slice from CRDT store.", id)
		// stateBytes is already []byte{}, so no change needed, stateNotFound remains false.
	}

	// Handle cases where one part is missing
	if metaNotFound && stateNotFound {
		opLog.Warnf("FlowObject %s not found in either base or CRDT store", id)
		return nil, datastore.ErrNotFound // Consistent not found error
	} else if metaNotFound {
		// State exists but metadata doesn't - indicates inconsistency
		return nil, fmt.Errorf("inconsistent FlowObject %s: state found in CRDT store but metadata missing from base store", id)
	} else if stateNotFound {
		// Metadata exists but state doesn't - might be valid if state can be empty/nil
		opLog.Warnf("FlowObject %s state not found in CRDT store, but metadata exists. Returning object with nil state.", id)
		// stateBytes was already set to nil when stateNotFound was determined
	}

	// 3. Deserialize metadata
	var meta flowObjectMetadata
	err = json.Unmarshal(metaBytes, &meta)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal metadata for FlowObject %s: %w", id, err)
	}

	// Basic validation
	if meta.ID != id {
		return nil, fmt.Errorf("loaded metadata ID mismatch: expected %s, got %s", id, meta.ID)
	}

	// 4. Construct the FlowObject
	obj := &types.FlowObject{
		ID:         meta.ID,
		ObjectType: meta.ObjectType,
		Metadata:   meta.Metadata,
		Sequence:   meta.Sequence,
		State:      stateBytes, // Assign loaded state
	}

	opLog.Infof("Successfully loaded FlowObject %s (State: %d bytes, Meta: %d bytes)", obj.ID, len(obj.State), len(metaBytes))
	return obj, nil
}
