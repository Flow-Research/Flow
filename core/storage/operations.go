package storage

import (
	"context"
	"encoding/json"
	"fmt"

	"flow-network/flow/core/types"

	crdt "github.com/ipfs/go-ds-crdt"
	datastore "github.com/ipfs/go-datastore"
	logging "github.com/ipfs/go-log/v2"
)

var opLog = logging.Logger("flow-storage-ops")

// GenerateKeyFromObjectID creates a datastore key suitable for the CRDT store from a FlowObject ID.
// NOTE: The CRDT store manages its own internal namespacing within the base datastore.
// This key is used directly with the CRDT store's Get/Put methods.
func GenerateKeyFromObjectID(id types.ObjectId) datastore.Key {
	// Using the CID string directly as the key within the CRDT namespace.
	// Ensure the key is valid for the datastore (e.g., non-empty).
	if id == types.UndefObjectId {
		// Handle or return error for undefined ID
		return datastore.NewKey("") // Or return an error
	}
	return datastore.NewKey(id.String())
}

// SaveFlowObject serializes and saves a FlowObject to the CRDT datastore.
// It takes the CRDT datastore instance directly.
func SaveFlowObject(ctx context.Context, crdtStore *crdt.Datastore, obj *types.FlowObject) error {
	if obj == nil {
		return fmt.Errorf("cannot save nil FlowObject")
	}
	if obj.ID == types.UndefObjectId {
		// TODO: Decide how to handle ID generation if not pre-set.
		// For now, assume ID is set before saving.
		return fmt.Errorf("cannot save FlowObject with undefined ID")
	}

	key := GenerateKeyFromObjectID(obj.ID)
	if key.String() == "" {
		return fmt.Errorf("generated empty key for FlowObject ID %s", obj.ID)
	}
	opLog.Debugf("Saving FlowObject with key: %s", key)

	// Serialize the entire object to JSON for this example.
	// In a real scenario, you might only store the CRDT state or use a more
	// efficient serialization format like CBOR or Protobuf.
	valueBytes, err := json.Marshal(obj)
	if err != nil {
		return fmt.Errorf("failed to marshal FlowObject %s: %w", obj.ID, err)
	}

	// Put the serialized object into the CRDT store.
	// go-ds-crdt handles the underlying CRDT logic and replication via the broadcaster.
	err = crdtStore.Put(ctx, key, valueBytes)
	if err != nil {
		return fmt.Errorf("failed to put FlowObject %s into CRDT store: %w", obj.ID, err)
	}

	opLog.Infof("Successfully saved FlowObject %s", obj.ID)
	return nil
}

// LoadFlowObject retrieves and deserializes a FlowObject from the CRDT datastore by its ID.
// It takes the CRDT datastore instance directly.
func LoadFlowObject(ctx context.Context, crdtStore *crdt.Datastore, id types.ObjectId) (*types.FlowObject, error) {
	if id == types.UndefObjectId {
		return nil, fmt.Errorf("cannot load FlowObject with undefined ID")
	}

	key := GenerateKeyFromObjectID(id)
	if key.String() == "" {
		return nil, fmt.Errorf("generated empty key for FlowObject ID %s", id)
	}
	opLog.Debugf("Loading FlowObject with key: %s", key)

	// Get the serialized object from the CRDT store.
	valueBytes, err := crdtStore.Get(ctx, key)
	if err != nil {
		if err == datastore.ErrNotFound {
			opLog.Warnf("FlowObject %s not found in CRDT store", id)
			return nil, err // Return specific error
		}
		return nil, fmt.Errorf("failed to get FlowObject %s from CRDT store: %w", id, err)
	}

	// Deserialize the object from JSON.
	var obj types.FlowObject
	err = json.Unmarshal(valueBytes, &obj)
	if err != nil {
		return nil, fmt.Errorf("failed to unmarshal FlowObject %s: %w", id, err)
	}

	// Basic validation
	if obj.ID != id {
		// This might happen if the stored data is corrupted or represents a different object
		return nil, fmt.Errorf("loaded object ID mismatch: expected %s, got %s", id, obj.ID)
	}

	opLog.Infof("Successfully loaded FlowObject %s", id)
	return &obj, nil
}
