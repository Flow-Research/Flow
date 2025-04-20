package infra

import (
	"context"
	"fmt"

	badgerds "github.com/ipfs/go-ds-badger3" // Use badger3
	datastore "github.com/ipfs/go-datastore"
	crdt "github.com/ipfs/go-ds-crdt"
	logging "github.com/ipfs/go-log/v2"

	libp2p "github.com/libp2p/go-libp2p"
	host "github.com/libp2p/go-libp2p/core/host"
	pubsub "github.com/libp2p/go-libp2p-pubsub"

	bserv "github.com/ipfs/boxo/blockservice"
	blockstore "github.com/ipfs/boxo/blockstore"
	offline "github.com/ipfs/boxo/exchange/offline"
	ipld "github.com/ipfs/go-ipld-format"
	merkledag "github.com/ipfs/boxo/ipld/merkledag"
)

var log = logging.Logger("crdt-setup")

// CRDTServices holds the initialized components needed for CRDT operation.
type CRDTServices struct {
	CrdtStore    *crdt.Datastore
	BaseStore    datastore.Datastore
	PubSub       *pubsub.PubSub
	Host         host.Host
	DAGService   ipld.DAGService
	BlockService bserv.BlockService
}

// BadgerOptions holds configuration for the base Badger datastore.
type BadgerOptions struct {
	Path string
}

// Libp2pOptions holds configuration for libp2p.
type Libp2pOptions struct {
	ListenAddrs []string // Example: []string{"/ip4/0.0.0.0/tcp/0"}
	// Add other options: Identity, Peerstore, etc.
}

// SetupCRDT initializes the base datastore, libp2p, pubsub, and the crdt datastore.
func SetupCRDT(ctx context.Context, badgerOpts BadgerOptions, p2pOpts Libp2pOptions) (*CRDTServices, error) {
	log.Info("Setting up CRDT services...")

	// 1. Initialize Base Datastore (Badger)
	log.Infof("Initializing Badger datastore at %s", badgerOpts.Path)
	dsOpts := badgerds.DefaultOptions
	// Badger3 options are slightly different, ensure Truncate/other defaults are suitable
	// dsOpts.Truncate = true // Not directly available in badger3 options struct
	baseDs, err := badgerds.NewDatastore(badgerOpts.Path, &dsOpts) // Using badger3's NewDatastore
	if err != nil {
		return nil, fmt.Errorf("failed to create base badger datastore: %w", err)
	}

	// 2. Initialize Libp2p Host
	// --- THIS IS A PLACEHOLDER --- 
	// You need to implement proper libp2p host creation based on your needs,
	// including identity generation/loading, transport configuration, etc.
	log.Info("Initializing Libp2p host (placeholder)...")
	// Example: Replace with actual libp2p host creation
	h, err := libp2p.New(
		// libp2p.ListenAddrStrings(p2pOpts.ListenAddrs...),
		// libp2p.Identity( /* your peer identity */ ),
		// libp2p.Transport( /* specific transports */ ),
		// libp2p.Security( /* security protocols */ ),
		// ... other options
	)
	if err != nil {
		return nil, fmt.Errorf("failed to create libp2p host (placeholder): %w", err)
	}
	log.Infof("Libp2p Host created with ID: %s", h.ID())
	log.Infof("Connect to this node: %s", h.Addrs())
	// --- END PLACEHOLDER ---

	// 3. Initialize PubSub
	log.Info("Initializing Libp2p PubSub...")
	ps, err := pubsub.NewGossipSub(ctx, h)
	if err != nil {
		return nil, fmt.Errorf("failed to create pubsub: %w", err)
	}

	// 4. Initialize DAGService (Required by go-ds-crdt v0.6+)
	log.Info("Initializing IPLD DAGService...")
	// Use the same Badger datastore for the blockstore
	// Note: Badger may not be the ideal blockstore, but suitable for basic setup.
	// Consider bs.NewBlockstore(baseDs) if blockstore interface matches
	// Creating a simple blockstore on top of the datastore for now.
	dsBlockstore := blockstore.NewBlockstore(baseDs)
	// Create a BlockService
	blockService := bserv.New(dsBlockstore, offline.Exchange(dsBlockstore))
	// Create a DAGService
	dagService := merkledag.NewDAGService(blockService)

	// 5. Initialize CRDT Datastore
	log.Info("Initializing CRDT datastore...")
	crdtNamespace := datastore.NewKey("crdt")
	// Options no longer contain Broadcaster directly
	crdtOpts := crdt.DefaultOptions()
	crdtOpts.Logger = logging.Logger("go-ds-crdt")
	// Create broadcaster separately, handling error
	pubsubBroadcaster, err := crdt.NewPubSubBroadcaster(ctx, ps, "flow-crdt-updates")
	if err != nil {
		baseDs.Close()
		h.Close()
		return nil, fmt.Errorf("failed to create pubsub broadcaster: %w", err)
	}

	// Call crdt.New with required arguments: ds, ns, dag, broadcaster, opts
	crdtStore, err := crdt.New(baseDs, crdtNamespace, dagService, pubsubBroadcaster, crdtOpts)
	if err != nil {
		baseDs.Close()
		h.Close()
		return nil, fmt.Errorf("failed to create crdt datastore: %w", err)
	}

	log.Info("CRDT services initialized successfully.")
	services := &CRDTServices{
		CrdtStore:    crdtStore,
		BaseStore:    baseDs,
		PubSub:       ps,
		Host:         h,
		DAGService:   dagService,
		BlockService: blockService,
	}

	return services, nil
}

// Close cleanly shuts down the CRDT services.
func (s *CRDTServices) Close() error {
	log.Info("Closing CRDT services...")
	var firstErr error
	if err := s.CrdtStore.Close(); err != nil {
		log.Errorf("Error closing CRDT store: %v", err)
		firstErr = err
	}
	// BlockService does not have a Close method in the typical Boxo setup
	// Closing the underlying BaseStore handles cleanup.
	// if err := s.BlockService.Close(); err != nil { ... }
	if err := s.BaseStore.Close(); err != nil {
		log.Errorf("Error closing base datastore: %v", err)
		if firstErr == nil {
			firstErr = err
		}
	}
	if err := s.Host.Close(); err != nil {
		log.Errorf("Error closing libp2p host: %v", err)
		if firstErr == nil {
			firstErr = err
		}
	}
	// PubSub doesn't typically need explicit closing, relies on Host close
	log.Info("CRDT services closed.")
	return firstErr
}
